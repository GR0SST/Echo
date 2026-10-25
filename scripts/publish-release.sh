#!/usr/bin/env bash
# Static update host: GR0SST/echo-update-host (private).
set -euo pipefail
[[ $# == 1 && -d $1 ]] || { echo "Usage: $0 <release-dir>" >&2; exit 2; }
release_dir=$(cd "$1" && pwd)
[[ -f $release_dir/manifest.json ]] || { echo 'Missing manifest.json' >&2; exit 2; }
[[ -z $(find "$release_dir" -type l -print -quit) ]] || { echo 'Symlinks are not permitted' >&2; exit 2; }
[[ -z $(find "$release_dir" ! -type f ! -type d -print -quit) ]] || { echo 'Only regular files and directories are permitted' >&2; exit 2; }
host=${ECHO_UPDATE_SSH_HOST:-echo-updates-deploy}
channel=${ECHO_UPDATE_CHANNEL:-stable}
keep=${ECHO_UPDATE_KEEP:-5}
[[ $host =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || exit 2
[[ $channel == stable || $channel == test ]] || exit 2
[[ $keep =~ ^[0-9]+$ && $keep -ge 2 && $keep -le 100 ]] || exit 2
id=$(date -u +%Y%m%d%H%M%S)-$(openssl rand -hex 8)
ssh_opts=(-o BatchMode=yes -o ConnectTimeout=15)
ssh "${ssh_opts[@]}" "$host" "umask 077; mkdir /srv/echo-updates/incoming/$id"
# Staging is not in a web root. No --delete ever targets published files.
rsync -rlt --chmod=Du=rwx,Dgo=,Fu=rw,Fgo= --safe-links \
    -e 'ssh -o BatchMode=yes -o ConnectTimeout=15' \
    "$release_dir/" "$host:/srv/echo-updates/incoming/$id/"
ssh "${ssh_opts[@]}" "$host" "/usr/local/libexec/echo-promote-release $channel $id $keep"
