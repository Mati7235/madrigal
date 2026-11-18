#!/bin/sh
echo "$@" >> "$FAKE_YTDLP_LOG"
case "$FAKE_MODE" in
  fail_default)
    case "$*" in
      *player_client*) ;;
      *)
        echo "ERROR: [youtube] Sign in to confirm you are not a bot" >&2
        exit 1
        ;;
    esac
    ;;
  rate_limited)
    echo "ERROR: HTTP Error 429: Too Many Requests" >&2
    exit 1
    ;;
esac
for last in "$@"; do :; done
video_id=$(printf '%s' "$last" | sed 's/.*v=//')
cat <<EOF
{"id": "$video_id", "title": "Fake Title", "duration": 200.0, "ext": "m4a", "acodec": "mp4a.40.2", "abr": 128.0, "url": "https://rr1---fake.googlevideo.com/videoplayback?expire=9999999999&id=abc", "http_headers": {"User-Agent": "fake-agent/1.0"}}
EOF
