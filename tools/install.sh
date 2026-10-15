#!/usr/bin/env bash
# Build an isolated preview. Stable installations and their data stay in place.
# Usage: tools/install.sh [--fast] [destination]
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
profile=release
cargo_args=()
if [ "${1:-}" = "--fast" ]; then
  profile=fast
  cargo_args=(-- --profile fast)
  shift
fi
dest="${1:-$HOME/.local/bin/code-ui-preview}"
if [ "$(basename -- "$dest")" = code-ui ]; then
  echo "Choose a preview destination such as ~/.local/bin/code-ui-preview. The stable binary is preserved." >&2
  exit 1
fi
cd "$root"
npm run tauri build -- --config src-tauri/tauri.preview.conf.json --no-bundle "${cargo_args[@]}"
install -d -- "$(dirname -- "$dest")"
if [ -f "$dest" ]; then
  cp -p -- "$dest" "$dest.previous-$(date -u +%Y%m%dT%H%M%SZ)"
fi
install -m 755 "src-tauri/target/$profile/code-ui" "$dest.new"
mv -f -- "$dest.new" "$dest"
echo "Installed isolated preview: $dest"
echo "Preview settings and conversations use code-ui-preview directories."
