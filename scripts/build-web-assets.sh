#!/usr/bin/env bash
# Builds everything the web app serves from web/public/:
#   wasm/{text,image}.wasm  — the tool packs
#   wasm/manifests.json     — tool catalog + coercion matrix (from core, via the CLI)
#   chains/                 — the community chain library + index
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release --target wasm32-unknown-unknown -p toolkit-pack-text -p toolkit-pack-image

mkdir -p web/public/wasm web/public/chains
cp target/wasm32-unknown-unknown/release/toolkit_pack_text.wasm web/public/wasm/text.wasm
cp target/wasm32-unknown-unknown/release/toolkit_pack_image.wasm web/public/wasm/image.wasm

cargo run --quiet --release -p toolkit-cli -- manifests > web/public/wasm/manifests.json

# Chain library: copy chains and build an index of their filenames.
rm -f web/public/chains/*.json
index="["
sep=""
for f in chains/*.json; do
    name="$(basename "$f")"
    cp "$f" "web/public/chains/$name"
    index="$index$sep\"$name\""
    sep=","
done
echo "$index]" > web/public/chains/index.json

echo "web assets ready:"
ls -la web/public/wasm web/public/chains
