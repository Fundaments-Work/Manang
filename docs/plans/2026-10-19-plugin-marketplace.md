# Plugin marketplace (hosted index, in the style of Obsidian's community plugins)

## What users get

Settings-free, in-app: **Plugins** page with two tabs, **Installed** and **Browse**. Browse lists the registry's plugins with search, tags and sort (name, newest, downloads). A plugin has a detail page: description, author, version, size, licence, tags, repository link, readme, changelog and, most important, **what it is allowed to do** (its declared capabilities). Install asks for consent on those capabilities (the existing consent flow). Installed plugins show Disable/Enable, **Update** (with a new consent if the permissions grew) and **Uninstall** (confirm, moved to the trash, as today). Updates are checked when the page opens and at most once a day in the background; nothing installs without a click.

## How Obsidian does it, and what we copy

Obsidian keeps a public GitHub repo with one JSON list of community plugins (id, name, author, description, repo). Authors submit by pull request, reviewers check the code, and the app fetches plugin files from the author's GitHub release assets. Cheap, transparent, no server. We keep the review-by-PR model and the author-owned releases, and add what a sandboxed platform makes possible and Obsidian lacks: **pinned hashes, a signed index, and capability disclosure**.

## Architecture

```
author repo (GitHub release: name-1.2.0.manangplugin)
        │ PR adds/updates registry/plugins/<id>.toml
        ▼
registry repo (this repo's registry/ folder, later its own repo)
  CI (cheap, path-filtered): validate entry, download the release asset,
  check manifest == entry, compute sha256, size, permissions
        │ on merge to main (manual dispatch): build-index.mjs
        ▼
index.json + index.json.sig (ed25519)  +  mirrored artifacts
        ▼
hosted: Cloudflare Worker + R2  (static, cached, Range/ETag, CORS)
        ▼
app: fetch index → verify signature → cache → browse → download artifact → verify sha256 → existing install + consent
```

* **Trust root:** the app embeds the registry's ed25519 **public key**. An index without a valid signature is rejected (the cache is kept, a notice says why). Every version carries `sha256` and `size`; the app refuses a mismatch and installs nothing. The signing key lives only as a CI secret of the registry; rotating means shipping a new app key list (several keys may be embedded).
* **Artifacts are mirrored** to the registry's R2 after review, so a version cannot change or vanish when an author deletes a release. `download` points at the mirror; `source` records the author's release URL.
* **Review criteria** (documented for authors): valid manifest; capabilities minimal and explained; open source with a licence; no obfuscated code; builds from the linked source; no network host that is not declared. Plugins run in the Wasmtime sandbox with declared capabilities (ADR 0003), so review is about honesty and quality, not safety of the host.
* **Privacy:** the app sends no identifiers; fetching the index is a plain GET. Download counts are counters per plugin in the Worker (no IP or user data stored). A setting `plugins.registry-url` overrides the registry (self-hosting, enterprise policy can pin it); `plugins.browse` can be switched off by policy.
* **Offline:** the last verified index is cached; Browse says "showing the list from <date>" and Install needs the network.
* **Web version later:** the same index and artifacts (CORS enabled), installed into the browser sandbox.

## Frozen index format, schema 1 (`index.json`)

```json
{
  "schema": 1,
  "generated": "2026-10-19T08:00:00Z",
  "plugins": [
    {
      "id": "word-count",                       // = manifest [plugin].name, kebab-case, unique
      "name": "Word count",
      "author": "Fundaments.work",
      "description": "Counts words, characters and lines in the open document.",
      "repo": "https://github.com/Fundaments-Work/manang-word-count",
      "homepage": "https://example.org",       // optional
      "license": "MIT",                         // SPDX
      "tags": ["writing", "statistics"],
      "tier": "wasm",                           // wasm (sandboxed); native plugins are first-party only and not listed
      "readme": "plugins/word-count/README.md", // path under the registry base URL, optional
      "latest": "0.3.1",
      "versions": [
        {
          "version": "0.3.1",
          "published": "2026-10-10T12:00:00Z",
          "min_app": "0.1.0",                   // optional
          "download": "plugins/word-count/0.3.1/word-count-0.3.1.manangplugin",
          "source": "https://github.com/.../releases/download/v0.3.1/word-count-0.3.1.manangplugin",
          "size": 48211,
          "sha256": "<hex>",
          "permissions": { "fs.read": "project", "network": ["api.example.org"] },  // exactly the manifest's capability declaration
          "changelog": "Fixes counting of CJK text."
        }
      ]
    }
  ]
}
```

`stats.json` (separate, optional): `{ "downloads": { "<id>": 123 } }`. All paths are relative to the registry base URL (`MANANG_REGISTRY_URL`, default placeholder `https://registry.manang.invalid` until the lead sets the host). `index.json.sig` is the base64 ed25519 signature over the exact bytes of `index.json`. Entries in the repo are `registry/plugins/<id>.toml`: `id`, `repo`, `tags`, `readme` (optional), `release_asset_pattern`; everything else is read from the release's manifest by the build script, never typed by hand.

## Work packages

1. **Registry repo scaffold and hosted index** (agent M2): schemas, validation, build and sign scripts with offline tests, sample entries, the Worker + R2 serving index, artifacts and stats, workflow (path-filtered, manual), publishing docs.
2. **Client and UI** (agent M1): index client (fetch, verify, cache, search), install/update from index through the existing consent and install paths, update checks, the Plugins page with Installed and Browse tabs, plugin detail page, uninstall confirm, mocks, tests, docs.
3. Lead: choose the host, generate the signing key (script provided), set `MANANG_REGISTRY_URL` and the embedded public key, deploy the Worker, seed the first entries (first-party plugins).

## Out of scope for the first version

Paid plugins, ratings and reviews, automatic security scanning beyond manifest checks, native (non-sandboxed) third-party plugins, per-user accounts.
