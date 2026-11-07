#!/bin/sh
# Regenerate tests/fixtures/luce-oracle.txt: build the luce-obj oracle (the
# Luce toolchain, `luc`, fetches luce-obj and what it needs from the
# registry at the versions luc.lock pins), write the corpus, run luce-obj on
# every file and fingerprint its output. The differential test compares the
# obj module against this file.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/obj-oracle.XXXXXX")
trap 'rm -rf "$work"' EXIT
(cd "$root/tools/luce-oracle" && luc sync && luce-base build src/main.lucb --native --release -o "$work/oracle")
mkdir "$work/corpus"
cargo run --quiet --release --manifest-path "$root/Cargo.toml" -p kinogaki-obj --example corpus -- write "$work/corpus"
for file in "$work"/corpus/*.obj; do
    out=$("$work/oracle" canon "$file" "$file.canon")
    case "$out" in ERROR*) printf '%s\n' "${out#ERROR }" > "$file.error" ;; esac
done
cargo run --quiet --release --manifest-path "$root/Cargo.toml" -p kinogaki-obj --example corpus -- fingerprints "$work/corpus" \
    > "$root/tests/fixtures/luce-oracle.txt"
echo "wrote tests/fixtures/luce-oracle.txt"
