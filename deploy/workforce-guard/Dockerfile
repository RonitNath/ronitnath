# Reuse the verified complete production application; change only auth boundaries.
FROM ghcr.io/ronitnath/ronitnath-app@sha256:b1cbe94cfcb43e500c61485f6ee3b6a7ba7c44962c633d229c34faa3e96b214a
COPY guard.cjs /app/isoastra-workforce-guard.cjs
COPY patch.cjs /tmp/patch-workforce-guard.cjs
RUN node /tmp/patch-workforce-guard.cjs
