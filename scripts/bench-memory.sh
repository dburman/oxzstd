#!/bin/bash
# Peak resident memory (MiB) of one-shot compression and decompression, oxzstd against the
# reference CLI, at a range of levels: scripts/bench-memory.sh FILE...
# Needs `cargo build --release --examples` first, and macOS's /usr/bin/time -l.
set -eu
ox=target/release/examples
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
rss() {
    /usr/bin/time -l "$@" 2>"$tmp/time" >/dev/null
    awk '/maximum resident set size/ { printf "%.1f", $1 / 1048576 }' "$tmp/time"
}
printf '%-14s %5s %10s %10s %10s %10s\n' input level ox-comp zstd-comp ox-dec zstd-dec
for f in "$@"; do
    for level in -5 -1 1 3 5 7 9 12 16 19 22; do
        if [ "$level" -lt 0 ]; then args=(--fast=${level#-})
        elif [ "$level" -gt 19 ]; then args=(--ultra "-$level")
        else args=("-$level"); fi
        "$ox/compress" "$level" "$f" "$tmp/ox.zst"
        oc=$(rss "$ox/compress" "$level" "$f" "$tmp/ox.zst")
        od=$(rss "$ox/decompress" "$tmp/ox.zst" "$tmp/ox.out")
        zc=$(rss zstd -q -f "${args[@]}" "$f" -o "$tmp/z.zst")
        zd=$(rss zstd -q -f -d "$tmp/z.zst" -o "$tmp/z.out")
        printf '%-14s %5s %10s %10s %10s %10s\n' "$(basename "$f")" "$level" "$oc" "$zc" "$od" "$zd"
    done
done
