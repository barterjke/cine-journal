# CI/CD

One workflow, `.github/workflows/ci-cd.yml`. It tests every change, and on `main` it
deploys the frontend to Vercel and the API to an Oracle Cloud ARM VM.

First-time VM and DNS setup is in [DEPLOY.md](../DEPLOY.md). This file covers what happens
on each push after that.

## Triggers

| Event | Tests | Deploys |
| --- | --- | --- |
| Pull request | yes | no |
| Push to `main` | yes | yes, if tests pass |
| Push to other branch | no | no |
| Manual **Run workflow** on `main` | yes | yes, if tests pass |

Jobs: `test-backend`, `test-frontend`, `deploy-api`, `deploy-frontend`.

Both deploy jobs declare `needs: [test-backend, test-frontend]`, so a failing test stops
both. They also check the branch and event — `needs:` alone is satisfied by a pull request
run, which would then deploy it.

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
  broken bundle can typecheck clean.
- `npm run test` is `vitest run`. It is a hard gate, not `--if-present`: if the script goes
  missing the job fails with a message saying so, rather than passing with no tests.

## How the API deploy works

1. `build-image` builds `backend/Dockerfile` twice in parallel — `linux/amd64` on
   `ubuntu-latest`, `linux/arm64` on `ubuntu-24.04-arm` (context = repo root). Each pushes
   to GHCR **by digest, untagged**.
2. `publish-image` combines both digests into one manifest list and attaches the tags
   `sha-<short>` and `latest`. It refuses to publish if fewer than two digests arrived.
3. Write the SSH key and known_hosts from secrets.
4. On the VM: `git fetch origin main && git reset --hard <sha>`, updating
   `docker-compose.yml` and `Caddyfile`. Cannot touch `.env` (gitignored) or the database
   (Docker volume). Does discard hand-edits to tracked files.
5. Write `.env` on the VM from `API_DOMAIN`, `ACME_EMAIL` and `TMDB_TOKEN`, over stdin.
   Skipped if the first two are unset, which leaves a hand-made `.env` alone.
6. Log the VM in to GHCR with the run's `GITHUB_TOKEN`, piped over stdin. Nothing
   long-lived is stored on the VM; the SSH key is the only standing credential.
7. `docker compose pull`, then `up -d --remove-orphans --wait --wait-timeout 330`, with
   `API_TAG` set to the sha tag. `--wait` blocks until the compose healthcheck passes.
8. Curl `API_HEALTH_URL`, then log the VM out of GHCR.

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
| `VERCEL_TOKEN` | API token | Vercel → Account Settings → Tokens |
| `VERCEL_ORG_ID` | Account/team id | `.vercel/project.json` after `vercel link`, or Settings → General |
| `VERCEL_PROJECT_ID` | Project id | Same file, or Project → Settings → General |

`GITHUB_TOKEN` is automatic. The job requests `packages: write` and uses it to push to
GHCR and to let the VM pull. It expires when the job ends.

### Setting them from the CLI

Same result as the web UI. Run from the repo root:

```bash
gh secret set OCI_HOST --body <VM_PUBLIC_IP>
gh secret set OCI_USER --body ubuntu
gh secret set OCI_SSH_KEY < ~/.ssh/oci-cine-journal      # path to your private key
ssh-keyscan <VM_PUBLIC_IP> | gh secret set OCI_SSH_KNOWN_HOSTS

gh secret set VERCEL_TOKEN        # prompts, so the token stays out of your shell history
gh secret set VERCEL_ORG_ID
gh secret set VERCEL_PROJECT_ID
```

The three Vercel ones are only needed if you keep the `deploy-frontend` job — see
*Vercel CLI vs git integration* below.

Check what's set with `gh secret list`. You can't read a secret back, only overwrite it.

Make sure `gh` is acting as the account that owns the repo — `gh auth status`, and
`gh auth switch -u <user>` if not. Secrets set on the wrong repo fail silently as far as
the workflow is concerned: it just reports the secret as missing.

## Variables

Settings → Secrets and variables → Actions → Variables. All optional.

| Name | Default | Purpose |
| --- | --- | --- |
| `API_HEALTH_URL` | none | Public URL of `GET /api/health`. Set it — it is the only check on Caddy, DNS, the certificate and the firewalls. Unset means a warning and a pass. |
| `API_DOMAIN` | none | The API's hostname. With the `ACME_EMAIL` secret, the deploy writes the VM's `.env` from these instead of you keeping it by hand. Either both or neither. |
| `DEPLOY_PATH` | `cine-journal` | The VM's git clone, relative to the deploy user's home. Matches DEPLOY.md. |
| `SSH_PORT` | `22` | Only if you moved sshd. |

Variables are a separate tab from secrets, and a separate command:

```bash
gh variable set API_HEALTH_URL --body https://api.yourdomain.com/api/health
gh variable list
```

## Setup checklist

1. Run through DEPLOY.md: VM, Docker, clone, `cp .env.deploy.example .env`, DNS.
2. Set the seven secrets above.
3. Set `API_HEALTH_URL`.
4. Put your real API domain in `frontend/vercel.json` (see limitations below).
5. Push to `main` once so the check names exist.
6. Add branch protection.

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

### API — revert (preferred)

```bash
git revert <bad-sha>
git push
```

Slower, but git stays the record of what is deployed.

### API — retag on the VM (fast)

```bash
ssh <user>@<host>
cd cine-journal
API_TAG=sha-abc1234 docker compose up -d
```

Get the tag from the run summary of the last good `deploy-api`, or from the package's
version list on GitHub. `docker-compose.yml` reads
`image: ghcr.io/barterjke/cine-journal-api:${API_TAG:-latest}`.

The variable is **`API_TAG`**. An unrecognised name does not error — compose falls back to
`:latest`, the image you were trying to get away from. If a rollback seems to do nothing,
check the variable name.

This leaves the VM running something `main` does not describe. The next push to `main`
undoes it, so land the revert too.

### Frontend

Vercel keeps every deployment. Deployments list → **Promote to Production**, or
`vercel rollback <url> --token=...`. Instant, no build.

## Failure modes

| Message | Cause and fix |
| --- | --- |
| `frontend/package.json` has no `"test"` script | The `test` script was removed or renamed. Restore it — the job will not silently skip tests. |
| `the lock file needs to be updated but --locked was passed` | `backend/Cargo.toml` changed without committing `Cargo.lock`. Run `cargo check` and commit it. |
| `denied: permission_denied` on push | GHCR package not linked to this repo. Package settings → Manage Actions access → add `cine-journal` with **Write**. Happens once. |
| `Host key verification failed` | `OCI_SSH_KNOWN_HOSTS` missing, wrong, or captured for a different port. Re-run `ssh-keyscan`. Also fires if you rebuilt the VM. |
| `Load key ".../deploy_key": invalid format` | `OCI_SSH_KEY` was pasted truncated, or is missing the `BEGIN`/`END` lines. |
| `permission denied ... Docker daemon socket` | `OCI_USER` is not in the `docker` group. `sudo usermod -aG docker $USER`, then reconnect. |
| `... is not a git checkout` | No clone at `$HOME/cine-journal`. Run DEPLOY.md Part 3, or set `DEPLOY_PATH`. |
| `neither .env.deploy nor .env exists` | VM never configured. `cp .env.deploy.example .env` and fill in `API_DOMAIN` and `TMDB_TOKEN`. |
| `container ... is unhealthy` after ~5 min | `--wait` gave up. Run `docker compose logs api` on the VM. Usually a startup panic or a `/data` volume the container's uid cannot write. |
| ssh step fails at 15s | VM down, IP changed, or port 22 closed. |
| Smoke test fails but containers are healthy | Firewall. Oracle images have iptables rules plus the OCI security list, and both must allow 80 and 443. See DEPLOY.md. |

## Half-failed deploys

The two deploy jobs are independent, so there are four outcomes.

| Outcome | What to do |
| --- | --- |
| Both pass | Nothing. |
| Both fail | Usually a missing secret. Nothing shipped. |
| Frontend passed, API failed | New UI against old API. Fix forward if the cause was infrastructure; otherwise roll the frontend back from Vercel. Risk depends on whether the commit changed `models.rs` and `api.ts` together. |
| API passed, frontend failed | Safer case. Old UI against new API. Re-run the failed job. |

They are not chained on purpose: chaining would let a Vercel outage block an API fix and
would not remove the mixed state anyway. To remove it for good, serve the frontend from
Caddy on the same VM.

## Two choices

**Native runners vs QEMU.** The image is built for both amd64 and arm64, each on its own
native runner, then merged into a manifest list. One runner with
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

**Vercel CLI vs git integration.** The CLI is used so the frontend is gated on the same
tests as the API, and so the bundle CI verified is the one that ships. The git integration
needs no workflow and gives free previews, but deploys whether or not tests passed. Do not
enable both — two deploys race for the production alias; if you switch, delete
`deploy-frontend`. Root Directory in the dashboard must be **empty** for the CLI (it already
runs in `frontend/`, so `frontend` makes it look for `frontend/frontend`) and `frontend` for
the git integration.

## Dependabot

`.github/dependabot.yml` opens weekly grouped pull requests for Cargo, npm and actions. They
run the same test jobs, so a breaking bump shows up before you merge it.

Actions are pinned to major tags, so minor and patch updates are accepted automatically. Pin
full SHAs to lock that down.

## Limitations

- **`frontend/vercel.json` ships pointing at `https://api.example.com`.** CI cannot catch
  this: both jobs pass, then every screen loads and fails to fetch. Replace both
  `destination` values with your API domain.
- **The API has no authentication.** Write endpoints take no credentials, by design — one
  shared visitor. `README.md` says not to expose it publicly; this workflow does. Decide
  deliberately before pointing a domain at it.
- **A green deploy does not mean the TMDB token works.** An invalid token is a supported
  state: the API serves the demo dataset with a banner. Check `GET /api/status` after the
  first deploy and after any token rotation.
