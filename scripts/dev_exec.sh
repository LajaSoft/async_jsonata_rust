#!/usr/bin/env bash

set -euo pipefail

workdir="."

usage() {
  cat <<'EOF' >&2
Usage: scripts/dev_exec.sh [-w <dir>] -- <command> [args...]

Run a command inside the dev container, forwarding stdin/stdout/stderr.
The command can be supplied inline or via here-documents.

Options:
  -w, --workdir DIR   Repository-relative directory to use as working dir
                      inside the container (default: .)
  -h, --help          Show this help message

Examples:
  scripts/dev_exec.sh -- node -e "console.log('hi')"
  scripts/dev_exec.sh -w src/jsonata-js-rust -- pnpm test
  scripts/dev_exec.sh -w src/jsonata-js-rust -- node <<'JS'
    console.log('hello from stdin');
  JS
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -w|--workdir)
      if [[ $# -lt 2 ]]; then
        echo "error: --workdir requires a value" >&2
        exit 1
      fi
      workdir="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
    ;;
    -*)
      echo "error: unknown option: $1" >&2
      usage
      exit 1
      ;;
    *)
      break
      ;;
  esac
done

if [[ $# -eq 0 ]]; then
  echo "error: no command specified" >&2
  usage
  exit 1
fi

cmd=()
for arg in "$@"; do
  cmd+=("$arg")
done

# Normalise workdir relative to repository root.
if [[ "$workdir" == "." ]]; then
  container_workdir="/workspace"
else
  # remove leading ./ if present
  workdir="${workdir#./}"
  if [[ "$workdir" = /* ]]; then
    container_workdir="$workdir"
  else
    container_workdir="/workspace/$workdir"
  fi
fi

# Build command string with proper shell escaping.
escaped_cmd=""
for arg in "${cmd[@]}"; do
  escaped_cmd+=" $(printf '%q' "$arg")"
done
escaped_cmd="${escaped_cmd# }"

docker compose run --rm -i \
  --workdir "$container_workdir" \
  dev bash -lc "export PATH=/opt/rust/bin:\$PATH; $escaped_cmd"
