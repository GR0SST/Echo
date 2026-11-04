#!/usr/bin/env bash
# Download the pinned public media fixture used by the native CEF gates.
set -euo pipefail
DIR="$HOME/Library/Caches/EchoCEFQualification/fixtures"
FILE="$DIR/sintel-trailer.mp4"
SHA256=b670602fa00934ca27c4351bb0efe7ea7a07fae57284e44226025eeed7c51254
mkdir -p "$DIR"
if [[ ! -f "$FILE" ]]; then
  curl -fL --retry 3 https://media.w3.org/2010/05/sintel/trailer.mp4 -o "$FILE.part"
  mv "$FILE.part" "$FILE"
fi
echo "$SHA256  $FILE" | shasum -a 256 -c -
