#!/usr/bin/env bash
set -euo pipefail
baseline=sha256:619bc050cdec3cfe593b32a905c35a8612d8753aa861517ca3456e4bd5cbd912
actual=$(docker image inspect ghcr.io/ronitnath/ronitnath-app@sha256:b1cbe94cfcb43e500c61485f6ee3b6a7ba7c44962c633d229c34faa3e96b214a --format '{{.Id}}')
[ "$actual" = "$baseline" ] || { echo 'Verified native Ronit baseline changed; refuse' >&2; exit 1; }
docker run --rm -v "$PWD:/proof:ro" --entrypoint node ghcr.io/ronitnath/ronitnath-app@sha256:b1cbe94cfcb43e500c61485f6ee3b6a7ba7c44962c633d229c34faa3e96b214a /proof/test.cjs
docker build -t ronitnath:workforce-subject-guard .
