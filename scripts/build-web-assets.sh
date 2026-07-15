#!/usr/bin/env bash
# Builds everything the web app serves from web/public/:
#   wasm/{text,image}.wasm  — the tool packs
#   wasm/manifests.json     — tool catalog + coercion matrix (from core, via the CLI)
#   chains/                 — the community chain library + index
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release --target wasm32-unknown-unknown -p toolkit-pack-text -p toolkit-pack-image -p toolkit-pack-crypto -p toolkit-pack-data -p toolkit-pack-math

mkdir -p web/public/wasm web/public/chains
cp target/wasm32-unknown-unknown/release/toolkit_pack_text.wasm web/public/wasm/text.wasm
cp target/wasm32-unknown-unknown/release/toolkit_pack_image.wasm web/public/wasm/image.wasm
cp target/wasm32-unknown-unknown/release/toolkit_pack_crypto.wasm web/public/wasm/crypto.wasm
cp target/wasm32-unknown-unknown/release/toolkit_pack_data.wasm web/public/wasm/data.wasm
cp target/wasm32-unknown-unknown/release/toolkit_pack_math.wasm web/public/wasm/math.wasm

cargo run --quiet --release -p toolkit-cli -- manifests > web/public/wasm/manifests.json

# Integrity manifest: sha256 of each pack, so the loader can verify the
# bytes it fetches before instantiating them. Pairs with reproducible
# builds — anyone can rebuild a pack and confirm the pinned hash.
# Written into src/ (not public/) so the pins are imported into the app
# bundle at build time: a runtime-fetched copy could be served from a
# different deploy than the app that checks it and fail spuriously.
sha256_hex() { # portable: coreutils sha256sum or BSD/macOS shasum
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
    else shasum -a 256 "$1" | cut -d' ' -f1; fi
}
{
    printf '{'
    sep=""
    for m in text image crypto data math; do
        printf '%s"%s.wasm":"%s"' "$sep" "$m" "$(sha256_hex "web/public/wasm/$m.wasm")"
        sep=","
    done
    printf '}\n'
} > web/src/lib/wasm-integrity.json
rm -f web/public/wasm/integrity.json # old fetched location; never ship a stale copy

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
