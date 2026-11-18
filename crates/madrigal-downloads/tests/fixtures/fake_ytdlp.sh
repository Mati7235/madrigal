#!/bin/sh
echo "$@" >> "$FAKE_YTDLP_LOG"
out=""
prev=""
for arg in "$@"; do
  case "$prev" in
    -o) out="$arg" ;;
  esac
  prev="$arg"
done
case "$FAKE_MODE" in
  fail*) exit 1 ;;
esac
file=$(printf '%s' "$out" | sed 's/%(ext)s/opus/')
mkdir -p "${file%/*}"
printf 'fake-audio' > "$file"
exit 0
