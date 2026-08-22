#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST_DIR="$SCRIPT_DIR/../portalis/rust/backend"

if cargo tree --manifest-path "$RUST_DIR/Cargo.toml" -i aws-lc-sys -e features >/dev/null 2>&1; then
  echo "[ERROR] iOS TLS graph still includes aws-lc-sys" >&2
  exit 1
fi

RUSTLS_FEATURES="$(cargo tree --manifest-path "$RUST_DIR/Cargo.toml" -e features -i rustls)"
if ! grep -Fq 'rustls feature "ring"' <<<"$RUSTLS_FEATURES"; then
  echo "[ERROR] iOS TLS graph does not enable Rustls ring provider" >&2
  exit 1
fi

if grep -Fq 'rustls feature "aws-lc-rs"' <<<"$RUSTLS_FEATURES"; then
  echo "[ERROR] iOS TLS graph still enables Rustls AWS-LC provider" >&2
  exit 1
fi

echo "iOS TLS graph uses Rustls ring without AWS-LC"
