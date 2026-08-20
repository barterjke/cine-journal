# CI/CD

One workflow, `.github/workflows/ci-cd.yml`. It tests every change, and on `main` it
builds two images and deploys both to one Oracle Cloud ARM VM.

First-time VM and DNS setup is in [DEPLOY.md](../DEPLOY.md). This file covers what happens
on each push after that.

## Triggers

| Event | Tests | Deploys |
| --- | --- | --- |
| Pull request | yes | no |
| Push to `main` | yes | yes, if tests pass |
| Push to other branch | no | no |
| Manual **Run workflow** on `main` | yes | yes, if tests pass |

Jobs: `test-backend`, `test-frontend`, `build-image`, `publish-image`, `build-web-image`,
`deploy`.

`build-image` and `build-web-image` both declare `needs: [test-backend, test-frontend]`, so
a failing test stops everything downstream. They also check the branch and event —
`needs:` alone is satisfied by a pull request run, which would then deploy it.

`deploy` needs both image jobs, so the bundle and the API always ship together.

## What the gates run

```
# test-backend (134 tests, ~0.1s, no network or Redis)
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-features --locked

# test-frontend
npm ci && npm run typecheck && npm run test && npm run build
```

- No `cargo fmt --check` gate: default rustfmt disagrees with this crate in ~3,600 lines. To
  add it, run `cargo fmt`, commit that alone, then add the step.
- `npm run build` is included because `tsc --noEmit` does not resolve asset imports, so a
  broken bundle can typecheck clean and then fail inside the web image build, after the API
  has already shipped.
- `npm run test` is `vitest run`. It is a hard gate, not `--if-present`: if the script goes
  missing the job fails with a message saying so, rather than passing with no tests.

## The two images

| Image | Built by | Contents |
| --- | --- | --- |
| `ghcr.io/barterjke/cine-journal-api` | `build-image` + `publish-image` | The Rust binary, plus `reference/cine-journal/img`. |
| `ghcr.io/barterjke/cine-journal-web` | `build-web-image` | nginx plus the built Vite bundle. ~93 MB, almost all of it the nginx base. |

Both get the tags `sha-<short>` and `latest`, from the same commit, so one sha rolls both
back.

The API needs a per-arch matrix and a manifest merge; the web image does not. `npm run
build` emits the same static files whichever arch is targeted, so `frontend/Dockerfile`
pins its node stage to `--platform=$BUILDPLATFORM` and one `docker/build-push-action` with
`platforms: linux/amd64,linux/arm64` covers both. There is no `setup-qemu-action` either,
because the nginx stage is `COPY`-only — buildkit never has to execute an arm64 binary.
**Add a `RUN` to that stage and the job needs `docker/setup-qemu-action@v3`**, or it fails
with `exec format error`.

## How the deploy works

1. `build-image` builds `backend/Dockerfile` twice in parallel — `linux/amd64` on
   `ubuntu-latest`, `linux/arm64` on `ubuntu-24.04-arm` (context = repo root). Each pushes
   to GHCR **by digest, untagged**.
2. `publish-image` combines both digests into one manifest list and attaches the tags
   `sha-<short>` and `latest`. It refuses to publish if fewer than two digests arrived.
   `build-web-image` runs alongside all of this and tags directly — one job, so there is no
   race for a tag to fix.
3. Write the SSH key and known_hosts from secrets.
4. On the VM: `git fetch origin main && git reset --hard <sha>`, updating
   `docker-compose.yml` and `Caddyfile`. Cannot touch `.env` (gitignored) or the database
   (Docker volume). Does discard hand-edits to tracked files.
5. Write `.env` on the VM from `API_DOMAIN`, `ACME_EMAIL` and `TMDB_TOKEN`, over stdin.
   Skipped if the first two are unset, which leaves a hand-made `.env` alone.
6. Log the VM in to GHCR with the run's `GITHUB_TOKEN`, piped over stdin. Nothing
   long-lived is stored on the VM; the SSH key is the only standing credential.
7. `docker compose pull`, then `up -d --remove-orphans --wait --wait-timeout 330`, with
   `API_TAG` and `WEB_TAG` set to the sha tag. `--wait` blocks until both healthchecks
   pass: `/api/health` on the API, `/healthz` on nginx.
8. Curl `API_HEALTH_URL`, then `https://$API_DOMAIN/` and grep it for `id="root"`, then log
   the VM out of GHCR.

Compose comes from the VM's git clone, not `scp`, so the clone stays clean and `git pull`
keeps working. The VM needs `git` and repo read access.

## Secrets

Settings → Secrets and variables → Actions → Secrets.

| Name | Value | Where to get it |
| --- | --- | --- |
| `OCI_HOST` | VM public IP or hostname | OCI console → Instances |
| `OCI_USER` | SSH login | `ubuntu` (Canonical) or `opc` (Oracle Linux) |
| `OCI_SSH_KEY` | Private key, whole PEM including `BEGIN`/`END` | The `.key` file the OCI console gave you when you created the instance. Its `.pub` is already in the VM's `authorized_keys`. Or make a new pair with `ssh-keygen -t ed25519 -f deploy_key` and append the `.pub` yourself. |
| `OCI_SSH_KNOWN_HOSTS` | VM host key | `ssh-keyscan -p 22 <ip>` |
| `ACME_EMAIL` | Your email, for Let's Encrypt expiry warnings | Optional. Set it with `API_DOMAIN` and the deploy writes the VM's `.env`; leave both unset and it uses whatever `.env` is on the box. |
| `TMDB_TOKEN` | TMDB v4 read access token | Optional. Only used when the deploy writes `.env`. Unset means demo mode plus a banner. |

`GITHUB_TOKEN` is automatic. The job requests `packages: write` and uses it to push to
GHCR and to let the VM pull. It expires when the job ends.

### Setting them from the CLI

Same result as the web UI. Run from the repo root:

```bash
gh secret set OCI_HOST --body <VM_PUBLIC_IP>
gh secret set OCI_USER --body ubuntu
gh secret set OCI_SSH_KEY < ~/.ssh/oci-cine-journal      # path to your private key
ssh-keyscan <VM_PUBLIC_IP> | gh secret set OCI_SSH_KNOWN_HOSTS

gh secret set ACME_EMAIL          # prompts, so nothing lands in your shell history
gh secret set TMDB_TOKEN
```

Check what's set with `gh secret list`. You can't read a secret back, only overwrite it.

Make sure `gh` is acting as the account that owns the repo — `gh auth status`, and
`gh auth switch -u <user>` if not. Secrets set on the wrong repo fail silently as far as
the workflow is concerned: it just reports the secret as missing.

## Variables

Settings → Secrets and variables → Actions → Variables. All optional.

| Name | Default | Purpose |
| --- | --- | --- |
| `API_HEALTH_URL` | none | Public URL of `GET /api/health`. Set it — it is the only check on Caddy, DNS, the certificate and the firewalls. Unset means a warning and a pass. |
| `API_DOMAIN` | none | The hostname, for the site and the API both. Does two jobs: with the `ACME_EMAIL` secret the deploy writes the VM's `.env` from these instead of you keeping it by hand (either both or neither), and the site smoke test curls `https://$API_DOMAIN/`. |
| `DEPLOY_PATH` | `cine-journal` | The VM's git clone, relative to the deploy user's home. Matches DEPLOY.md. |
| `SSH_PORT` | `22` | Only if you moved sshd. |

Variables are a separate tab from secrets, and a separate command:

```bash
gh variable set API_HEALTH_URL --body https://api.yourdomain.com/api/health
gh variable list
```

## Setup checklist

1. Run through DEPLOY.md: VM, Docker, clone, `cp .env.deploy.example .env`, DNS.
2. Set the six secrets above.
3. Set `API_HEALTH_URL` and `API_DOMAIN`.
4. Push to `main` once so the check names exist.
5. Add branch protection.

## Branch protection

Until you do this, a red pull request can still be merged.

Settings → Branches → Add branch ruleset:

1. Target `main`.
2. Enable **Require status checks to pass**.
3. Add **`Backend tests`** and **`Frontend tests`**. These names only appear after the
   workflow has run once.
4. Optionally enable **Require branches to be up to date before merging**.
5. Do **not** add the deploy jobs. They never run on a pull request, so requiring them
   blocks every merge permanently.

## Rolling back

### Revert (preferred)

```bash
git revert <bad-sha>
git push
```

Slower, but git stays the record of what is deployed.

### Retag on the VM (fast)

```bash
ssh <user>@<host>
cd cine-journal
API_TAG=sha-abc1234 WEB_TAG=sha-abc1234 docker compose up -d
```

Both images carry the same sha tag, so one value does the whole stack. Drop whichever
variable you don't need — `up -d` only recreates containers whose image changed, so rolling
back just the UI leaves the API and its database untouched.

Get the tag from the run summary of the last good `deploy`, or from the packages' version
lists on GitHub.

The variables are **`API_TAG`** and **`WEB_TAG`**. An unrecognised name does not error —
compose falls back to `:latest`, the image you were trying to get away from. If a rollback
seems to do nothing, check the variable name.

This leaves the VM running something `main` does not describe. The next push to `main`
undoes it, so land the revert too.

## Failure modes

| Message | Cause and fix |
| --- | --- |
| `frontend/package.json` has no `"test"` script | The `test` script was removed or renamed. Restore it — the job will not silently skip tests. |
| `the lock file needs to be updated but --locked was passed` | `backend/Cargo.toml` changed without committing `Cargo.lock`. Run `cargo check` and commit it. |
| `denied: permission_denied` on push | GHCR package not linked to this repo. Package settings → Manage Actions access → add `cine-journal` with **Write**. Once per package, so `cine-journal-web` needs it too the first time `build-web-image` runs. |
| `Host key verification failed` | `OCI_SSH_KNOWN_HOSTS` missing, wrong, or captured for a different port. Re-run `ssh-keyscan`. Also fires if you rebuilt the VM. |
| `Load key ".../deploy_key": invalid format` | `OCI_SSH_KEY` was pasted truncated, or is missing the `BEGIN`/`END` lines. |
| `permission denied ... Docker daemon socket` | `OCI_USER` is not in the `docker` group. `sudo usermod -aG docker $USER`, then reconnect. |
| `... is not a git checkout` | No clone at `$HOME/cine-journal`. Run DEPLOY.md Part 3, or set `DEPLOY_PATH`. |
| `neither .env.deploy nor .env exists` | VM never configured. `cp .env.deploy.example .env` and fill in `API_DOMAIN` and `TMDB_TOKEN`. |
| `container ... is unhealthy` after ~5 min | `--wait` gave up. Run `docker compose logs api` on the VM. Usually a startup panic or a `/data` volume the container's uid cannot write. |
| `did not serve the bundle` | The API answered but the site didn't, so DNS and the certificate are fine. Either `web` is down (`docker compose logs web`) or Caddy's catch-all `handle` block is wrong. |
| Site loads, refresh 404s | The SPA fallback. `try_files $uri $uri/ /index.html` in `frontend/nginx.conf`, and check the image was actually rebuilt. |
| Blank page after a deploy, old bundle in devtools | A cached `index.html`. It should be served `no-cache`; check with `curl -sI https://<host>/`. |
| ssh step fails at 15s | VM down, IP changed, or port 22 closed. |
| Smoke test fails but containers are healthy | Firewall. Oracle images have iptables rules plus the OCI security list, and both must allow 80 and 443. See DEPLOY.md. |

## Half-failed deploys

There is one deploy job now, needing both image jobs, so a build failure ships nothing.
This is the point of serving both from the same VM: there is no state where a new bundle
is live against an old API because one of two providers was down.

Compose still restarts `api` and `web` as separate containers, so a few seconds of skew
during `up -d` is possible. `api.ts` and the API are versioned together in the same
commit, and the window is short enough that a retry covers it.

## Two choices

**Native runners vs QEMU.** The *API* image is built for both amd64 and arm64, each on its
own native runner, then merged into a manifest list. One runner with
`docker/setup-qemu-action@v3` would be simpler YAML but emulates rustc, turning a ~6
minute build into most of an hour.

Both arches are built because which shape you get on the Always Free tier isn't fully your
choice: A1.Flex (arm64) is frequently out of capacity, and the fallback E2.1.Micro is
amd64. A single-arch image on the wrong shape pulls successfully and then dies with
`exec format error`, which looks like a corrupt binary rather than an architecture
mismatch.

Layer caching (`type=gha`) is scoped per arch — one shared scope would have each arch
evicting the other's layers every run. It only helps at all because `backend/Dockerfile`
builds dependencies in a layer the source doesn't invalidate; keep it that way.

**Baked bundle vs mounted directory.** The bundle is built into the `web` image rather than
built on the VM and bind-mounted. So a rollback is a tag, the VM needs no Node, and the
files that ship are the ones a build produced — not whatever is in a directory. The cost is
a ~93 MB image push per deploy, nearly all of it the unchanged nginx base, so the layer is
already on the VM.

## Dependabot

`.github/dependabot.yml` opens weekly grouped pull requests for Cargo, npm and actions. They
run the same test jobs, so a breaking bump shows up before you merge it.

Actions are pinned to major tags, so minor and patch updates are accepted automatically. Pin
full SHAs to lock that down.

## Limitations

- **Nothing here validates `Caddyfile` or `frontend/nginx.conf`.** Both are copied to the VM
  by `git reset --hard`, not built or linted. A bad `handle` block or a lost `try_files`
  passes every gate and breaks the site. The site smoke test catches the first case, not the
  second — test a deep link by hand after changing either file.
- **The API has no authentication.** Write endpoints take no credentials, by design — one
  shared visitor. `README.md` says not to expose it publicly; this workflow does, and now on
  the same hostname as the UI. Decide deliberately before pointing a domain at it.
- **A green deploy does not mean the TMDB token works.** An invalid token is a supported
  state: the API serves the demo dataset with a banner. Check `GET /api/status` after the
  first deploy and after any token rotation.
