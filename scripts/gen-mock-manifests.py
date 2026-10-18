#!/usr/bin/env python3
"""Regenerate src/lib/kernel/mockManifests.ts from crates/manang-*/manifest.toml.

The browser demo kernel shows each plugin's description, permissions and
settings exactly as the Rust manifests declare them. Run after changing a
manifest:  python3 scripts/gen-mock-manifests.py && pnpm exec prettier --write src/lib/kernel/mockManifests.ts
"""

import glob
import json
import tomllib

entries = []
for path in sorted(glob.glob("crates/manang-*/manifest.toml")):
    with open(path, "rb") as f:
        m = tomllib.load(f)
    entry = {
        "description": m["plugin"].get("description", ""),
        "permissions": m.get("permissions", {}),
    }
    settings = m.get("contributes", {}).get("settings", [])
    for s in settings:
        s.setdefault("scope", "user")
        s.setdefault("requires-restart", False)
    if settings:
        entry["settings"] = settings
    entries.append((m["plugin"]["name"], entry))

body = ",\n".join(
    f"  {json.dumps(n) if '-' in n else n}: {json.dumps(e, ensure_ascii=False)}"
    for n, e in entries
)
with open("src/lib/kernel/mockManifests.ts", "w") as out:
    out.write(
        f"""/**
 * The `[plugin]`, `[permissions]` and `[[contributes.settings]]` tables of the
 * first-party plugins' `manifest.toml`, for the in-memory kernel. Generated
 * by `scripts/gen-mock-manifests.py`; regenerate when a manifest changes
 * rather than editing by hand.
 */

import type {{ SettingDef }} from "./api";
import type {{ PluginPermissions }} from "./client";

export interface MockManifest {{
  readonly description: string;
  readonly permissions: PluginPermissions;
  readonly settings?: readonly Omit<SettingDef, "owner" | "category">[];
}}

export const MOCK_MANIFESTS: Readonly<Record<string, MockManifest>> = {{
{body},
}};
"""
    )
