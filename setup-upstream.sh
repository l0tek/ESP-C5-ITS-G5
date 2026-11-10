#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
REPO="$ROOT/upstream"
if [ -e "$REPO" ]; then echo "upstream/ already exists; refusing to overwrite" >&2; exit 1; fi
if ! git clone https://git.hamburg.ccc.de/jtbx/esp32-c_its-companion.git "$REPO"; then
  # The first clone is owned by this script and may have left a partial directory.
  rm -rf "$REPO"
  git clone https://codeberg.org/c-its/esp32-c_its-companion.git "$REPO"
fi
cd "$REPO"
# Freeze source at reviewed commit (2026-09-30) rather than tracking floating main.
git checkout dc54b55230
cp "$ROOT/firmware/src/main.rs" src/main.rs
python3 - <<'PY'
from pathlib import Path
p=Path('Cargo.toml')
s=p.read_text()
s=s.replace('default = ["esp", "spat", "gnss_ubx", "screen"]', 'default = ["esp"]')
assert 'default = ["esp"]' in s
p.write_text(s)
PY
printf '\nReady. cd upstream && cargo run --release\n'
