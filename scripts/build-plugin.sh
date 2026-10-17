#!/usr/bin/env bash
# Build a Manang plugin crate into an installable plugin directory.
#
#   scripts/build-plugin.sh <plugin-crate-dir> [out-dir]
#
# Produces <out-dir>/<name>/{manifest.toml,plugin.wasm}; out-dir defaults to
# <plugin-crate-dir>/dist. Copy the <name> directory into
# <config>/manang/plugins/ to install it.
#
# Needs: the wasm32-unknown-unknown target
#   rustup target add wasm32-unknown-unknown
set -euo pipefail

usage() {
  echo "usage: $0 <plugin-crate-dir> [out-dir]" >&2
  exit 2
}

[[ $# -ge 1 && $# -le 2 ]] || usage
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
plugin_dir="$(cd "$1" && pwd)"
out_root="${2:-$plugin_dir/dist}"

[[ -f "$plugin_dir/Cargo.toml" ]] || { echo "no Cargo.toml in $plugin_dir" >&2; exit 1; }
[[ -f "$plugin_dir/manifest.toml" ]] || { echo "no manifest.toml in $plugin_dir" >&2; exit 1; }

# The plugin name is the directory name the host expects.
name="$(sed -n 's/^name *= *"\(.*\)"/\1/p' "$plugin_dir/manifest.toml" | head -n 1)"
[[ -n "$name" ]] || { echo "manifest.toml has no plugin name" >&2; exit 1; }

# The cdylib's file name is the package name with '-' replaced by '_'.
package="$(sed -n '/^\[package\]/,/^\[/ s/^name *= *"\(.*\)"/\1/p' "$plugin_dir/Cargo.toml" | head -n 1)"
[[ -n "$package" ]] || { echo "Cargo.toml has no package name" >&2; exit 1; }
module="${package//-/_}.wasm"

target_dir="$plugin_dir/target"
cargo build --release --target wasm32-unknown-unknown \
  --manifest-path "$plugin_dir/Cargo.toml" --target-dir "$target_dir"

out="$out_root/$name"
mkdir -p "$out"
cargo run --quiet --release \
  --manifest-path "$repo_root/sdk/componentize/Cargo.toml" \
  --target-dir "$repo_root/sdk/componentize/target" -- \
  "$target_dir/wasm32-unknown-unknown/release/$module" "$out/plugin.wasm"
cp "$plugin_dir/manifest.toml" "$out/manifest.toml"

echo "built $out"
