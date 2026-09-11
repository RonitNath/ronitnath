# syntax=docker/dockerfile:1

FROM node:24-alpine@sha256:50c8e8ca1d27439048670df5883f32d57cf81cff6233222c893fd0d9884cbd81 AS deps
WORKDIR /app
RUN corepack enable
COPY package.json pnpm-lock.yaml .npmrc ./
RUN --mount=type=secret,id=npm_token,required=true \
    sh -eu -c 'cp .npmrc /tmp/build.npmrc; printf "\n//npm.pkg.github.com/:_authToken=%s\n" "$(cat /run/secrets/npm_token)" >> /tmp/build.npmrc; NPM_CONFIG_USERCONFIG=/tmp/build.npmrc pnpm install --frozen-lockfile; rm -f /tmp/build.npmrc'

FROM node:24-alpine@sha256:50c8e8ca1d27439048670df5883f32d57cf81cff6233222c893fd0d9884cbd81 AS build
WORKDIR /app
RUN corepack enable
ARG APP_VERSION=dev
ENV APP_VERSION=${APP_VERSION} NEXT_TELEMETRY_DISABLED=1
COPY --from=deps /app/node_modules ./node_modules
COPY . .
RUN pnpm build

# Bundle only the migration runner and its runtime dependencies. Keeping the
# full Next.js build toolchain out of this image materially reduces registry
# transfer and the preflight pull on each production host.
FROM deps AS migrate-build
COPY scripts/migrate.ts ./scripts/migrate.ts
COPY drizzle ./drizzle
RUN pnpm exec esbuild scripts/migrate.ts \
    --bundle --platform=node --format=cjs --target=node24 \
    --outfile=/migration/migrate.cjs

FROM node:24-alpine@sha256:50c8e8ca1d27439048670df5883f32d57cf81cff6233222c893fd0d9884cbd81 AS migrate
WORKDIR /app
COPY --from=migrate-build /migration/migrate.cjs ./migrate.cjs
COPY drizzle ./drizzle
CMD ["node", "migrate.cjs"]

FROM node:24-alpine@sha256:50c8e8ca1d27439048670df5883f32d57cf81cff6233222c893fd0d9884cbd81 AS runtime
WORKDIR /app
ARG APP_VERSION=dev
ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    APP_VERSION=${APP_VERSION} \
    PORT=3140 \
    HOSTNAME=0.0.0.0
RUN apk add --no-cache curl
COPY --from=build --chown=node:node /app/.next/standalone ./
COPY --from=build --chown=node:node /app/.next/static ./.next/static
COPY --from=build --chown=node:node /app/public ./public
# Migrations travel with the image; a one-off container applies them by hand.
COPY --from=build --chown=node:node /app/drizzle ./drizzle
USER node
EXPOSE 3140
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
  CMD curl -fsS "http://127.0.0.1:${PORT}/healthz" || exit 1
CMD ["node", "server.js"]
