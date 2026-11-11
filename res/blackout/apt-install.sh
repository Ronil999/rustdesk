#!/bin/bash
# apt-get install for CI runners that fails fast and retries instead of hanging on a stalled
# mirror: short network timeouts, three attempts, and a switch from the runner's Azure mirror
# to archive.ubuntu.com after the first failure.
#
# usage: apt-install.sh <package>...
set -uo pipefail

opts=(-o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30)
sudo_=""
[ "$(id -u)" -ne 0 ] && sudo_=sudo

for attempt in 1 2 3; do
  if timeout 600 $sudo_ apt-get "${opts[@]}" update -y &&
    timeout 1200 $sudo_ env DEBIAN_FRONTEND=noninteractive apt-get "${opts[@]}" install -y "$@"; then
    exit 0
  fi
  echo "apt attempt $attempt failed; retrying with archive.ubuntu.com" >&2
  $sudo_ sed -i 's|http://azure.archive.ubuntu.com|http://archive.ubuntu.com|g' \
    /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources 2>/dev/null || true
  sleep 20
done
echo "apt-install: giving up after 3 attempts" >&2
exit 1
