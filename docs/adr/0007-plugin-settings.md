# ADR 0007: Plugin settings: declared in the manifest, pushed through `<plugin>.configure`

Status: accepted, 2026-10-18

## Context

The settings plugin (`crates/manang-settings`) has a typed registry
(`settings.register`, `settings.schema`, `settings.get`), layered values
(defaults < user `settings.toml` < project `.manang/settings.toml`) and emits
`settings.changed`. Only the shell's core settings were registered, and no
value ever reached a plugin: the LaTeX engine, search excludes or spelling
language could only be set per request.

"Everything is a plugin", including third-party WASM components (ADR 0003),
which have no ambient filesystem and cannot read `settings.toml` themselves,
and which must not be granted a capability just to learn their own
configuration. A plugin should also not have to call `settings.register` at
start-up: that needs a handle to the kernel from inside the plugin, ordering
against the settings plugin, and a different code path for native and WASM.

## Decision

1. **Settings are declared in `manifest.toml`**, the single source of truth:

   ```toml
   [[contributes.settings]]
   id = "latex.engine"            # <plugin-name>.<kebab-case>
   type = "enum"                  # bool | int | float | enum | string | path | list
   values = ["pdflatex", "xelatex", "lualatex"]
   default = "pdflatex"           # required, validated against the type
   scope = "project"              # optional: user (default) | project
   title = "Engine"               # optional short UI label
   description = "TeX engine used to compile LaTeX documents."
   requires-restart = false       # optional
   ```

   The shape is the JSON shape of `manang_settings::SettingDef` (internally
   tagged `type`, `min`/`max`, `values`, `item`), so the kernel's
   `SettingSpec` converts losslessly. The kernel validates namespacing (same
   rule as commands), duplicates, at most `MAX_ENTRIES`, type parameters and
   the default when the manifest is parsed, so a broken third-party manifest
   is refused at load time, before any of it is registered.

2. **Values are pushed, not pulled.** A plugin that declares settings provides
   the command `<plugin>.configure` with input
   `{"values": {"<id>": <effective value>, ...}}`: every one of its settings,
   after layering. It applies them all or none and ignores ids it does not
   know (an older binary with a newer manifest). Per-request arguments still
   override the configured defaults.

3. **`manang_settings::PluginSettingsSync`** does the plumbing. For a plugin it
   registers the manifest settings (owner = plugin name, so the Settings UI
   groups them under the plugin) and pushes the values. A listener thread on a
   bounded `settings.*` subscription re-pushes the owning plugin's values on
   `settings.changed` (owner = id before the first dot) and every plugin's
   values on the new `settings.project-changed` event, which
   `settings.use-project` emits after swapping the project layer. Bursts are
   coalesced to one push per plugin. The listener holds only a `Weak` kernel
   and exits on `stop()`, on drop, or when the kernel or bus is dropped; it
   never blocks an emitter.

4. **The shell** syncs every first-party plugin after `build_kernel`, and each
   WASM plugin after the initial scan and on every load or hot reload. A
   same-owner re-registration with a changed definition replaces it, so a hot
   reloaded manifest can change its settings.

Only settings that change behaviour today were added: `latex.backend`,
`latex.engine`, `latex.latexmk`, `latex.timeout-seconds`; `search.exclude`,
`search.include-hidden`; `writing.spellcheck`, `writing.spell-language`,
`writing.format-width`; `citations.style`. Typst font options exist only as
construction-time compiler configuration, and git and markdown have no
plugin-wide switch, so they declare none yet.

## Consequences

- **WASM plugins** get settings with no new capability and no new WIT
  import: declaring them in the manifest and exporting `configure` through the
  existing command entry point is enough. Their values cross the boundary as
  the same JSON payload as any other command.
- **Web version:** the mechanism is plain kernel commands and events, with no
  filesystem access in the plugin and no OS dependency in the kernel types,
  so a browser kernel can provide the same `settings.*` surface (backed by
  browser storage) and drive the same `configure` calls.
- **Plugin state** is now mutable at runtime: plugins keep their configuration
  behind a lock and take a snapshot per request, so a change applies to the
  next request and never to one in flight.
- A plugin that rejects a pushed value keeps its previous configuration; the
  settings registry has already validated the type and range, so this only
  happens for constraints the registry cannot express (for example a
  dictionary that fails to load).
- Settings ids are owned by the plugin name: a plugin cannot declare or
  overwrite another plugin's or a core setting.
