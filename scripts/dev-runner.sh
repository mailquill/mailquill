#!/bin/sh
# Cargo runner for local development.
# Walks up from the script's location to find .env, loads it (without overriding
# vars already set in the environment), then executes the target binary.
set -e

script_dir=$(cd "$(dirname "$0")" && pwd)
dir="$script_dir"

while [ "$dir" != "/" ]; do
    if [ -f "$dir/.env" ]; then
        set -a
        # shellcheck disable=SC1090
        . "$dir/.env"
        set +a
        break
    fi
    dir=$(dirname "$dir")
done

exec "$@"
