# CinéJournal API image.
#
# Build from the repo root, not from backend/:
#
#     docker build -f backend/Dockerfile -t ghcr.io/barterjke/cine-journal-api .
#
# main.rs serves /img from ../reference/cine-journal/img, relative to the working
# directory. A backend/ context can't see those files, and the app still starts —
# it just 404s every avatar. So: root context, WORKDIR /app/backend, images at
# /app/reference/cine-journal/img. IMG_DIR itself is unchanged.

# ---------------------------------------------------------------------- build

# Bookworm, same as the runtime stage, so the binary gets the glibc it built
# against. -slim: this stage installs its own toolchain.
FROM rust:1.93-slim-bookworm AS build

# build-essential is needed: rusqlite's `bundled` feature compiles SQLite from C.
#
# Not needed, checked by building without them:
#   - pkg-config / libssl-dev. reqwest 0.13 defaults to rustls, not native-tls, so
#     there is no OpenSSL in the tree.
#   - cmake. rustls uses aws-lc-rs, and aws-lc-sys 0.43 falls back to a cc build
#     when CMake is missing. If a `cargo update` breaks that, the error is
#     "Missing dependency: cmake" and adding it here is the fix.
RUN apt-get update \
 && apt-get install -y --no-install-recommends build-essential \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app/backend

# Dependencies in their own layer, keyed on the manifests only, so editing a
# handler doesn't rebuild aws-lc-sys and libsqlite3-sys. The stub main.rs just
# gives cargo something to build them for.
#
# --locked: build from the committed Cargo.lock.
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir -p src \
 && echo 'fn main() {}' > src/main.rs \
 && cargo build --release --locked \
 && rm -rf src target/release/cine-journal-api \
           target/release/deps/cine_journal_api-* \
           target/release/.fingerprint/cine-journal-api-*

# The stub's artifacts are removed above rather than relying on mtimes, since COPY
# keeps the source timestamp and cargo could think this crate is already built.
COPY backend/src ./src
RUN cargo build --release --locked

# -------------------------------------------------------------------- runtime

FROM debian:bookworm-slim AS runtime

# ca-certificates is required. rustls uses the platform verifier, which reads
# /etc/ssl/certs; without it every TMDB call fails its handshake and the app falls
# back to the demo dataset. So the symptom is invented films, not an error.
#
# curl is the healthcheck client (see docker-compose.yml). debian-slim has none.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

# Fixed uid: it's baked into the /data volume's ownership below.
RUN groupadd --system --gid 10001 app \
 && useradd --system --uid 10001 --gid app --no-create-home app

# The database volume, and the only path the process needs to write. A new named
# volume inherits this ownership from the image; an existing one keeps its own, so
# a volume created by a root-running image needs a manual chown.
RUN mkdir -p /data && chown app:app /data
ENV DATABASE_PATH=/data/cine-journal.db

# Resolves to ../reference/cine-journal/img from the WORKDIR below. Only img/ —
# the rest of reference/ is mockups.
COPY --chown=app:app reference/cine-journal/img /app/reference/cine-journal/img

COPY --from=build /app/backend/target/release/cine-journal-api /usr/local/bin/cine-journal-api

# IMG_DIR and db::DEFAULT_PATH are both relative, so the working directory
# matters. Left root-owned: if DATABASE_PATH were ever unset, the app panics
# naming the path instead of writing the database into the container's own layer.
WORKDIR /app/backend

USER app

# Loopback is the binary's default and would be unreachable from outside the
# container. See DEFAULT_BIND_ADDR in main.rs.
ENV BIND_ADDR=0.0.0.0 \
    PORT=3001

EXPOSE 3001

# No .env is copied in and no TMDB_TOKEN is baked. dotenvy doesn't override
# variables already set, so compose's environment always wins.
CMD ["cine-journal-api"]
