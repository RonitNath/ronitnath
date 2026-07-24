#!/bin/sh
exec webdeploy --manifest "$(dirname "$0")/app.toml" "$@"
