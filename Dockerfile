# syntax=docker/dockerfile:1

FROM node:24-alpine AS deps
WORKDIR /app
RUN corepack enable
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

FROM node:24-alpine AS build
WORKDIR /app
RUN corepack enable
ARG APP_VERSION=dev
ENV APP_VERSION=${APP_VERSION} NEXT_TELEMETRY_DISABLED=1
COPY --from=deps /app/node_modules ./node_modules
COPY . .
RUN pnpm build

# Migrations are applied by hand from a one-off container: this stage keeps the
# full toolchain (drizzle-kit is a dev dependency) and never serves traffic.
FROM build AS migrate
CMD ["pnpm", "db:migrate"]

FROM node:24-alpine AS runtime
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
