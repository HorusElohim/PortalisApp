#!/usr/bin/env bash
set -euo pipefail

# Shared build/test parallelism. Callers may override each exported value.
portalis_cpu_count() {
  local count=""
  if command -v getconf >/dev/null 2>&1; then
    count="$(getconf _NPROCESSORS_ONLN 2>/dev/null || true)"
  fi
  if [[ -z "$count" ]] && command -v nproc >/dev/null 2>&1; then
    count="$(nproc 2>/dev/null || true)"
  fi
  if [[ -z "$count" ]] && command -v sysctl >/dev/null 2>&1; then
    count="$(sysctl -n hw.ncpu 2>/dev/null || true)"
  fi
  [[ "$count" =~ ^[1-9][0-9]*$ ]] || count=1
  printf '%s' "$count"
}

PORTALIS_CPU_COUNT="${PORTALIS_CPU_COUNT:-$(portalis_cpu_count)}"
export PORTALIS_CPU_COUNT
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$PORTALIS_CPU_COUNT}"
export PORTALIS_TEST_THREADS="${PORTALIS_TEST_THREADS:-$PORTALIS_CPU_COUNT}"
export PORTALIS_FLUTTER_TEST_CONCURRENCY="${FLUTTER_TEST_CONCURRENCY:-$PORTALIS_CPU_COUNT}"

# Gradle reads this JVM system property. Preserve caller-provided options.
gradle_parallelism="-Dorg.gradle.workers.max=${GRADLE_WORKERS_MAX:-$PORTALIS_CPU_COUNT}"
if [[ " ${GRADLE_OPTS:-} " != *" $gradle_parallelism "* ]]; then
  export GRADLE_OPTS="${GRADLE_OPTS:-} $gradle_parallelism"
fi

echo "==> parallelism: CPUs=$PORTALIS_CPU_COUNT cargo=$CARGO_BUILD_JOBS tests=$PORTALIS_TEST_THREADS flutter=$PORTALIS_FLUTTER_TEST_CONCURRENCY gradle=${GRADLE_WORKERS_MAX:-$PORTALIS_CPU_COUNT}" >&2
