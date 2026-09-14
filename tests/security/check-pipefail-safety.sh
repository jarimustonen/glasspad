#!/bin/bash
# Regression guard for test-security.sh's grep assertions.
set -euo pipefail

target="${1:-test-security.sh}"

# Demonstrate the failure mode this guard prevents. grep -q exits after its first
# match, so pipefail reports the producer's SIGPIPE (141), or its EPIPE error (1)
# when a wrapper made SIGPIPE ignored. Either is a failure despite a found match.
set +e
yes gp-pipefail-probe 2>/dev/null | grep -q gp-pipefail-probe
producer_status=$?
set -e
case "$producer_status" in
  141|1) ;;
  *)
    echo "expected an early grep match to make its producer fail, got $producer_status" >&2
    exit 1
    ;;
esac

# Assertions must feed grep through a file or redirection instead. This catches
# both direct producer | grep -q and producer | filter | grep -q forms used here.
unsafe_lines="$(awk '
  /^[[:space:]]*#/ { next }
  /\|.*grep[[:space:]]+-[^[:space:]]*q/ { print NR ":" $0 }
' "$target")"
if [ -n "$unsafe_lines" ]; then
  echo "unsafe grep -q pipeline(s) in $target:" >&2
  printf '%s\n' "$unsafe_lines" >&2
  exit 1
fi

echo "PASS  security gate grep assertions are pipefail-safe (early-exit producer status reproduced: $producer_status)"
