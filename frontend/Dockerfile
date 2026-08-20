# CinéJournal web image: the built Vite bundle, served by nginx.
#
# Build from frontend/, unlike backend/Dockerfile:
#
#     docker build -t ghcr.io/barterjke/cine-journal-web frontend
#
# Nothing outside this directory is an input, so the context is frontend/ and the
# ignore list is frontend/.dockerignore.
#
# Caddy is in front of this in production and proxies /api and /img to the API, so
# the bundle is same-origin with the API and nginx never needs to know it exists.
# See Caddyfile.

# ---------------------------------------------------------------------- build

# --platform=$BUILDPLATFORM pins this stage to the machine doing the build. Without
# it, a `--platform linux/arm64` build on an amd64 runner runs npm and node under
# QEMU. The output is a directory of static files, identical either way.
FROM --platform=$BUILDPLATFORM node:22-alpine AS build

WORKDIR /app

# Manifests first, so editing a component doesn't reinstall node_modules.
#
# .npmrc is required, not incidental: it pins registry.npmjs.org. Without it npm
# resolves against whatever registry the build environment defaults to.
COPY package.json package-lock.json .npmrc ./

# `ci` not `install`: installs the lockfile exactly and fails if it has drifted.
RUN npm ci

# `npm run build` is `tsc -b && vite build`, so it needs tsconfig.json,
# vite.config.ts and the Tailwind/PostCSS configs as well as src/.
COPY . .
RUN npm run build

# -------------------------------------------------------------------- runtime

# No RUN in this stage, on purpose. A stage that only copies files needs no process
# execution, so building it for linux/arm64 on an amd64 runner needs no QEMU and no
# per-arch matrix — see the build-web-image job in .github/workflows/ci-cd.yml.
#
# That is also why this runs as the image's default root: dropping privileges would
# need a chown of /var/cache/nginx, i.e. a RUN. nginx's master process is root but
# its workers run as uid 101 (`user nginx` in /etc/nginx/nginx.conf), the container
# publishes no ports, and it serves static files only.
FROM nginx:1-alpine AS runtime

# Replaces the image's default server block. SPA fallback and cache headers.
COPY nginx.conf /etc/nginx/conf.d/default.conf

COPY --from=build /app/dist /usr/share/nginx/html

EXPOSE 80
