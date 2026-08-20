# Deploying CinéJournal

Everything runs on one Oracle Cloud VM, in the free tier. Four containers, one open
port.

```
browser
   │  https://your-host/
   ▼
Oracle Cloud VM
  caddy :443  (TLS, the only open port)
    ├── /api/*  ─┐
    ├── /img/*  ─┴─→ api
    │                 ├── redis   (feed cache, no disk)
    │                 ├── /data   (SQLite, on a volume)
    │                 └── TMDB    (outbound)
    └── everything else → web  (nginx + the built Vite bundle)
```

One origin, so there is no CORS and nothing to rewrite. `api.ts` asks for `/api/...`
and the browser is already on the right host.

Two details that are easy to get wrong, both handled in `Caddyfile` and
`frontend/nginx.conf`:

- **`/img` is routed like `/api`.** TMDB posters are absolute CDN URLs, but friend
  avatars are files the API serves. Miss it and posters work while every avatar 404s.
- **nginx needs an SPA fallback.** The routes live in `frontend/src/main.tsx`, not on
  disk, so a reload on `/collections/favorites` has to be answered with
  `index.html`.

## Warning: writes are not authenticated

There are no accounts. One shared visitor. Anyone who opens the site can change your
ratings, watchlist, bio and comments — and the site and the API are one hostname now,
so the URL is not obscure.

This is how the app is built, not a bug. See `backend/src/state.rs`. If you don't
want it, see [Add a password](#add-a-password) below before you set up DNS.

## What you need

- Oracle Cloud account (free tier needs a card for ID checks, doesn't charge it)
- One hostname. A free `duckdns.org` subdomain is enough; you don't need to buy a
  domain. See Part 2.
- TMDB read access token. Without it the app serves fake data and shows a banner.

Examples below write the hostname as `api.example.com`. Substitute whatever you pick
in Part 2 — `yourname.duckdns.org`, say. It is the only hostname there is: the same
name serves the site and the API.

---

## 1. Oracle Cloud VM

### Region

Always Free resources only exist in your tenancy's **home region**, set at signup and
permanent. Pick a large region near you — small regions run out of free ARM capacity.

### Create the instance

Console → Compute → Instances → Create instance.

| Setting | Value |
|---|---|
| Image | Ubuntu 24.04, **aarch64** build |
| Shape | `VM.Standard.A1.Flex`, **2 OCPU / 12 GB** |
| Boot volume | default (~47 GB) |
| SSH key | paste your public key |

2 OCPU / 12 GB is the whole Always Free ARM allowance. Use it in one instance.

Block storage is 200 GB total across the tenancy. You don't need an extra volume —
the SQLite file is tiny and lives on a Docker volume on the boot disk.

**"Out of host capacity"** is normal, not a misconfiguration. Try each availability
domain, then retry later. Capacity frees up in minutes to days.

### Open ports 80 and 443 — in two places

There are two firewalls. Fixing one leaves the symptom unchanged.

**1. VCN**, in the console: Networking → Virtual Cloud Networks → your VCN → public
subnet → Security List → Add Ingress Rules.

| Source | Protocol | Source Port Range | Destination Port Range |
|---|---|---|---|
| `0.0.0.0/0` | TCP | **All** | 80 |
| `0.0.0.0/0` | TCP | **All** | 443 |

**Leave Source Port Range as `All`.** Putting 80 or 443 there is the easy mistake, because
the form has two port fields and one of them is the number you're thinking about. Clients
connect *from* a random high port *to* 80, so a rule with source port 80 matches nothing.
It looks correct in the rule list, and the symptom is a connection that hangs — including
Let's Encrypt reporting `Timeout during connect (likely firewall problem)`.

Compare against the default SSH rule, which ships with source port `All`. That is what
yours should look like.

**2. The instance**, over SSH. Oracle's Ubuntu images reject everything except SSH:

```bash
sudo iptables -I INPUT 6 -m state --state NEW -p tcp --dport 80 -j ACCEPT
sudo iptables -I INPUT 6 -m state --state NEW -p tcp --dport 443 -j ACCEPT
sudo netfilter-persistent save
```

Check with `sudo iptables -L INPUT --line-numbers` that the rules are **above** the
REJECT line. Below it they do nothing.

Skipping this gives you a connection that hangs then times out. It looks like a DNS
or cert problem. It isn't.

### Install Docker

```bash
sudo apt-get update && sudo apt-get install -y ca-certificates curl
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker "$USER"
```

Log out and back in, then test with `docker run --rm hello-world`.

### Add swap (optional)

Only needed if you build on the VM. The GitHub Actions workflow ships prebuilt
images, so normally you don't. Add it anyway — 12 GB is fine to run this, not to
compile it.

```bash
sudo fallocate -l 4G /swapfile && sudo chmod 600 /swapfile
sudo mkswap /swapfile && sudo swapon /swapfile
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

---

## 2. A hostname

One name, for the site and the API both. You need it because Let's Encrypt won't issue
a certificate for a bare IP, and without a certificate Caddy can't serve HTTPS. Pick
one:

### Option A — DuckDNS (free, no domain needed)

Go to [duckdns.org](https://www.duckdns.org), sign in with GitHub, pick a subdomain,
and put your VM's public IP in the box. That's the whole setup.

Then `API_DOMAIN=yourname.duckdns.org`.

Works because `duckdns.org` is on the Public Suffix List, so Let's Encrypt treats your
subdomain as its own domain with its own rate limit.

### Option B — your own domain (~$10/yr)

At your registrar, add one record:

| Type | Name | Value | TTL |
|---|---|---|---|
| `A` | `films` | your VM's public IP | 300 |

That gives you `films.yourdomain.com`, and that one record is all you need. If you'd
rather the app sat at the apex, point an `A` record for `@` at the same IP and use
that as `API_DOMAIN` instead.

### Don't use nip.io or sslip.io

They look ideal — `1.2.3.4.sslip.io` resolves to `1.2.3.4` with no signup. But neither
is on the Public Suffix List, so Let's Encrypt counts the whole of `sslip.io` as one
domain with a 50-certificates-per-week limit **shared with every other user in the
world**. Issuance fails unpredictably.

### Before you start Caddy

The name must already resolve. Caddy asks Let's Encrypt for a certificate on startup,
and Let's Encrypt proves ownership by connecting to whatever the name points at. If
it doesn't resolve yet the attempt fails, and repeated failures hit a rate limit that
keeps failing you for a while after you've fixed it.

```bash
dig +short yourname.duckdns.org     # must print your VM's IP
```

The hostname goes in exactly one place: `API_DOMAIN` in `.env` on the VM (Part 3).
Nothing is baked into the frontend bundle.

---

## 3. Deploy

**Everything in this part runs on the VM, not on your laptop.** SSH in first:

```bash
ssh -i ~/.ssh/oci-cine-journal ubuntu@<VM_PUBLIC_IP>
```

Then, on the VM:

```bash
git clone https://github.com/barterjke/cine-journal.git
cd cine-journal
cp .env.deploy.example .env
nano .env                      # fill in the three values below
docker compose pull            # see note
docker compose up -d
```

`.env` is gitignored, so it is never committed. Do it by hand for the first run — you
want to watch the logs once — then hand it to CI as below.

| Variable | Required | Purpose |
|---|---|---|
| `API_DOMAIN` | yes | Your hostname from Part 2, e.g. `cinema-nerd.duckdns.org`. Caddy gets its cert for this. No `https://`, no trailing slash. |
| `ACME_EMAIL` | yes | **A real address of yours.** Let's Encrypt rejects `example.com`, `test.com` and friends with `invalidContact`, and Caddy then falls back to a different certificate authority — so the failure shows up as "no HTTPS" rather than "bad email". |
| `TMDB_TOKEN` | no | v4 read access token. Empty = fake data + banner. |
| `API_TAG` | no | Tag of the API image. Defaults to `latest`. Used for rollback. |
| `WEB_TAG` | no | Tag of the web image. Same idea, rolls back independently. |

So a filled-in `.env` is three lines:

```
API_DOMAIN=cinema-nerd.duckdns.org
ACME_EMAIL=your.real.address@gmail.com
TMDB_TOKEN=eyJhbGciOi...
```

Do not leave `ACME_EMAIL` as an `@example.com` address. Let's Encrypt returns
`invalidContact - contact email has forbidden domain`, Caddy silently tries another
certificate authority instead, and the symptom you see is a site that never comes up —
nothing that mentions email. `docker compose logs caddy` is where it says so.

### Let CI own .env instead

Recommended once the first deploy works. Put the same three values in GitHub and every
deploy writes `.env` on the VM for you:

```bash
gh variable set API_DOMAIN --body cinema-nerd.duckdns.org
gh secret set ACME_EMAIL --body your.real.address@gmail.com
gh secret set TMDB_TOKEN                              # prompts
```

Why bother: Oracle stops idle Always Free instances, and ARM capacity churns, so you
will probably rebuild this VM at some point. A hand-made `.env` dies with it. With the
config in GitHub, a fresh VM needs only Docker, `git clone`, and a deploy.

Two things to know:

- **Hand edits get overwritten.** Once these are set, each deploy rewrites `.env`.
  Change the values in GitHub, not on the box.
- **Keep your TMDB token somewhere else too.** GitHub secrets are write-only — you
  cannot read one back, only replace it.

If `API_DOMAIN` or `ACME_EMAIL` is unset the deploy skips the write and leaves your
hand-made file alone, so the manual route keeps working.

`API_DOMAIN` and `ACME_EMAIL` have no defaults — compose refuses to start without
them. An empty `ACME_EMAIL` used to crash-loop Caddy with `wrong argument count`, so
it's now a hard requirement rather than an optional field.

**Run `docker compose pull` before `up -d`.** `api` and `web` both have `image:` and
`build:`, so if a tag isn't already local, `up` builds it instead — for `api` that's a
silent 15-minute Rust compile on two shared ARM cores.

**Name the file `.env`.** Docker Compose only auto-loads `.env`. Any other name needs
`--env-file` on every command, and forgetting it silently falls back to `:latest`
instead of erroring.

Don't set `REDIS_URL` yourself — compose sets it to `redis://redis:6379`. Setting it
to `127.0.0.1` looks right but points at the container's own loopback, and the cache
silently stops working.

### Check it worked

Run these in order. Each one narrows down the next failure.

```bash
docker compose ps                                    # all 4 up, api and web healthy
curl -s https://api.example.com/api/health           # Caddy + DNS + TLS + api
curl -s https://api.example.com/ | grep 'id="root"'  # the bundle
curl -s https://api.example.com/collections/favorites | grep 'id="root"'   # SPA fallback
docker compose logs api | grep -E 'tmdb|redis'       # tmdb: enabled / redis: enabled
```

The third and fourth must both return HTML. If the third works and the fourth 404s,
nginx is serving but `try_files` isn't — check `frontend/nginx.conf`.

The last one matters most. A bad token and a dead cache both leave you with a working
API and a quieter log, so a 200 from `/api/health` doesn't mean it's configured.

Neither `api` nor `web` publishes a host port, so `curl localhost:3001` no longer
works. Go through Caddy, or `docker compose exec api curl -s localhost:3001/api/health`
to skip it.

### First boot is slow

On an empty database the API fetches a social graph from TMDB before it starts
serving (`seed_graph` in `backend/src/main.rs`). If `docker compose ps` shows `api`
as `starting` for a while, that's this. It's non-fatal — if TMDB is down you get an
empty friend list, not a boot failure.

---

## 4. How the frontend is served

There is no separate frontend deploy. Part 3 started it: the `web` service is nginx
with the built Vite bundle baked in, and Caddy sends it everything that isn't `/api`
or `/img`.

### The three files

| File | Job |
|---|---|
| `frontend/Dockerfile` | `node:22-alpine` runs `npm ci && npm run build`, then `nginx:1-alpine` copies in `dist`. |
| `frontend/nginx.conf` | SPA fallback and cache headers. |
| `Caddyfile` | Path routing: `/api/*` and `/img/*` to `api:3001`, everything else to `web:80`. |

### SPA fallback

```nginx
try_files $uri $uri/ /index.html;
```

This is the line that matters. The app's routes — `/collections/favorites`,
`/people/elenarostova` — exist only in `frontend/src/main.tsx`. Nothing was built to
those paths, so without the fallback the first load works and every refresh 404s.

### Cache headers

- `/assets/*` — `max-age=31536000, immutable`. Vite hashes those filenames, so a
  changed file gets a new name.
- `index.html` — `no-cache`. It names the hashed bundles, so a cached copy points at
  files the last deploy deleted, and the app loads to a blank page.

### Path routing

`handle`, not `handle_path`. `handle_path` strips the matched prefix, so `/api/health`
would arrive at the API as `/health` and 404 the whole API.

Caddy sorts `handle` blocks by path specificity and puts the matcher-less one last, so
`/api/*` and `/img/*` beat the catch-all regardless of the order you write them in.
Check it with:

```bash
docker compose exec caddy caddy adapt --config /etc/caddy/Caddyfile
```

### No hostname in the bundle

`api.ts` fetches `/api/...` and `/img/...` — root-relative, so the browser asks
whatever host it is already on. Single-origin needs no code change, no CORS headers,
and no build-time API URL. The Vite dev proxy in `frontend/vite.config.ts` does the
same two prefixes locally.

### Building it by hand

```bash
docker compose build web        # or: docker build -t cine-journal-web frontend
```

Context is `frontend/`, unlike the API, whose context is the repo root because it
serves `/img` from `reference/`.

---

## Add a password

Optional. Read the warning at the top first.

If this is a personal journal on a domain nobody knows, doing nothing is reasonable —
the worst case is your own film ratings, and backups are one command away.

If not, put a password on writes only. Add to `Caddyfile`, no code changes:

```caddyfile
@writes method POST PUT DELETE
basic_auth @writes {
    # Generate with: docker run --rm caddy caddy hash-password
    you $2a$14$...your.hash.here...
}
```

Reads stay open. The browser prompts once per session, so the buttons still work.

This is not a login system. For more than one person you need real identity in the
API and a per-user cache key — see the `feed_key` comment in `backend/src/cache.rs`.

---

## Running it

### Update

GitHub Actions does this on every push to `main` — see [docs/ci-cd.md](docs/ci-cd.md).
By hand:

```bash
cd cine-journal && git pull
docker compose pull && docker compose up -d
```

### Back up

Your data is one SQLite file. Copy it with `.backup`, not `cp` — WAL mode keeps
recent commits in a sidecar file, so copying just the `.db` can miss your latest
changes.

```bash
docker compose exec api sqlite3 /data/cine-journal.db ".backup '/data/backup.db'"
docker compose cp api:/data/backup.db ./cine-journal-$(date +%F).db
```

If `sqlite3` isn't in the image, `docker compose stop api` first and copy the file
cold.

Redis needs no backup. It only holds cached feed pages, rebuilt from SQLite and TMDB
on any miss.

### Roll back

Both images are tagged `sha-<short>` by the same commit, so a full rollback is one
tag in two variables:

```bash
API_TAG=sha-abc1234 WEB_TAG=sha-abc1234 docker compose up -d
```

Either one alone works if only one half is broken. No `docker compose down` first —
`up -d` recreates just the containers whose image changed.

The names are `API_TAG` and `WEB_TAG`. Get one wrong and compose won't error — it uses
`latest`, so the rollback appears to do nothing. Take the tags from the deploy's run
summary; see `docs/ci-cd.md`.

### Logs

```bash
docker compose logs -f api
docker compose logs -f caddy   # cert problems are here, not in api
docker compose logs -f web     # nginx access log; a 404 here is a routing bug
```

`RUST_LOG=cine_journal_api=debug` adds per-operation cache hits and misses. Useful
for telling a missing cache from an unreachable one.

---

## Gotchas

**Oracle reclaims idle instances.** If CPU, network **and** memory 95th-percentile are
all under 20% for 7 days, Oracle may reclaim yours. A personal film journal fits that
profile.

Because it's all three, keeping one above 20% is enough. Memory is the practical one:
20% of 12 GB is ~2.4 GB, so set a Redis `maxmemory` around there. Network won't work —
20% of this shape's bandwidth is hundreds of Mbps, so health checks don't move it.

Get an uptime monitor (UptimeRobot, Better Stack) anyway, for the normal reason: one
VM, no redundancy. Just don't rely on it for this.

Reclaimed means **stopped**, not deleted. Your boot volume and database survive. You
restart from the console. Accepting that risk is fine for a personal project.

**TMDB rate limits are per token.** Building a feed page is one upstream call per
film. Redis amortises that over 10-minute windows; without it every request pays.
If films vanish from the feed, check the cache is up before blaming TMDB.

**Keep the `caddy_data` volume.** It holds your certs and ACME account key. Delete it
and you re-issue from scratch, against Let's Encrypt's rate limits.

**`docker compose down -v` deletes your database.** `-v` removes named volumes. Film
data comes back from TMDB; reviews, follows and ratings don't — they only ever existed
there.
