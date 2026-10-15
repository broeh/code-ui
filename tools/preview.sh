#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
export CODE_UI_PROFILE=preview
exec npm run tauri dev -- --config src-tauri/tauri.preview.conf.json "$@"
