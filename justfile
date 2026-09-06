set shell := ["bash", "-uc"]

# dev server against the compose database
dev:
    pnpm dev

# typecheck + lint + unit tests + build
gate:
    pnpm gate

db-up:
    docker compose -f compose.dev.yaml up -d

db-down:
    docker compose -f compose.dev.yaml down

db-generate:
    pnpm db:generate

db-migrate:
    pnpm db:migrate

e2e:
    pnpm exec playwright test

image tag=`git rev-parse --short HEAD`:
    docker build --build-arg APP_VERSION={{tag}} -t ghcr.io/ronitnath/ronitnath:{{tag}} .
