#!/usr/bin/env bash

set -euo pipefail

recover=0
workdir="src/jsonata-js-rust"
expression=""

usage() {
  cat <<'EOF' >&2
Usage: scripts/rust_parse.sh [options] [expression]

Evaluate an expression with the Rust parser via the native bridge.
If no expression argument is supplied, the script reads from stdin
so it can be used conveniently with here-documents.

Options:
  -r, --recover      Enable recover mode when parsing (default: disabled)
  -w, --workdir DIR  Repository-relative working directory when running
                     inside the container (default: src/jsonata-js-rust)
  -h, --help         Show this help message

Examples:
  scripts/rust_parse.sh '$match("test", /t/)'

  scripts/rust_parse.sh -r <<'EOF'
  function($x){$x}
  EOF
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -r|--recover)
      recover=1
      shift
      ;;
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
      expression="$1"
      shift
      ;;
  esac
done

if [[ -z "$expression" ]]; then
  expression="$(cat)"
fi

if [[ -z "$expression" ]]; then
  echo "error: no expression provided" >&2
  exit 1
fi

expr_b64="$(printf '%s' "$expression" | base64 | tr -d '\n')"

read -r -d '' js_script <<'NODE' || true
const native = require("./native/index.node");
const source = Buffer.from(process.env.JSONATA_SOURCE_B64, "base64").toString();
const recover = process.env.JSONATA_RECOVER === "1";

try {
  const result = native.parseExpression(source, recover);
  console.log(JSON.stringify(result, null, 2));
} catch (err) {
  console.error("Rust parser threw:");
  console.error(err);
  process.exit(1);
}
NODE

js_b64="$(printf '%s' "$js_script" | base64 | tr -d '\n')"

"$(dirname "$0")"/dev_exec.sh -w "$workdir" -- bash -lc "export JSONATA_SOURCE_B64='$expr_b64' JSONATA_RECOVER='$recover'; echo '$js_b64' | base64 -d | node"
