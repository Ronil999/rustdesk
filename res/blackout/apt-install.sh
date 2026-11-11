#!/bin/bash
# apt-get install for CI runners whose default Ubuntu mirror is unreachable. GitHub's images
# read their mirrors from /etc/apt/apt-mirrors.txt (azure.archive.ubuntu.com first); when that
# host stops resolving, apt waits out a timeout for every single package and the step hangs.
# So first probe the candidate mirrors, point apt only at ones that answer, then install with
# short timeouts and a few retries.
#
# usage: apt-install.sh <package>...
set -uo pipefail

sudo_=""
[ "$(id -u)" -ne 0 ] && sudo_=sudo
codename=$(. /etc/os-release && echo "$VERSION_CODENAME")

candidates=(
  http://azure.archive.ubuntu.com/ubuntu
  http://archive.ubuntu.com/ubuntu
  http://us.archive.ubuntu.com/ubuntu
  http://mirrors.edge.kernel.org/ubuntu
  http://mirror.math.princeton.edu/pub/ubuntu
)
good=()
for m in "${candidates[@]}"; do
  if curl -fsS -o /dev/null --connect-timeout 5 --max-time 15 "$m/dists/$codename/InRelease"; then
    good+=("$m")
  else
    echo "apt-install: mirror $m unreachable, skipping" >&2
  fi
done
if [ ${#good[@]} -eq 0 ]; then
  echo "apt-install: no Ubuntu mirror reachable" >&2
  exit 1
fi
echo "apt-install: using ${good[*]}"

if [ -f /etc/apt/apt-mirrors.txt ]; then
  for i in "${!good[@]}"; do
    printf '%s/\tpriority:%d\n' "${good[$i]}" $((i + 1))
  done | $sudo_ tee /etc/apt/apt-mirrors.txt >/dev/null
fi
for f in /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources; do
  [ -f "$f" ] || continue
  $sudo_ sed -i -E "s#https?://(azure\.)?archive\.ubuntu\.com/ubuntu/?#${good[0]}#g" "$f"
done

opts=(-o Acquire::Retries=3 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20)
for attempt in 1 2 3; do
  if timeout 300 $sudo_ apt-get "${opts[@]}" update -y &&
    timeout 900 $sudo_ env DEBIAN_FRONTEND=noninteractive apt-get "${opts[@]}" install -y "$@"; then
    exit 0
  fi
  echo "apt attempt $attempt failed" >&2
  sleep 15
done
echo "apt-install: giving up after 3 attempts" >&2
exit 1
