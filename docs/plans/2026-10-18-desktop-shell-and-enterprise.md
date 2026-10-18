# Desktop shell, contextual settings, plugin manager and enterprise readiness

Date: 2026-10-18. Asked for by the product owner: a professional desktop app
"ready to be used in enterprise settings": a menu bar in the title bar like
any desktop application, Settings that show only what belongs to where they
were opened, a visible plugin management system, and a figure studio and PDF
viewer at the level of Excalidraw/tldraw and Overleaf. Facts below come from a
survey of the code at main 6583ee3.

## Order of work

| # | Package | Branch | Depends on | Status |
| :--- | :--- | :--- | :--- | :--- |
| 1 | PDF viewer at Overleaf level (text selection, find, links, zoom, outline; Typst through the same viewer) | `agent/pdf-viewer` | | In progress |
| 2 | Figure studio (Excalidraw/tldraw class, Mermaid to shapes, agent tools) | `agent/figure-studio` | | In progress |
| 3 | Menu bar and one command/shortcut model | `agent/menu-bar` | | Next |
| 4 | Contextual Settings | `agent/context-settings` | | Next |
| 5 | Plugin manager (backend + Plugins view) | `agent/plugin-manager` | 2 merged (shared src-tauri, kernel) | After 1-2 |
| 6 | Enterprise policy layer | `agent/enterprise-policy` | 4, 5 | After 4-5 |
| 7 | Release engineering (installers, signing, updates, i18n, accessibility audit) | later | 6 | Later |

## 3. Menu bar and one command/shortcut model

Today: no menu bar; the window has its own title bar (`decorations: false`);
commands live in `src/lib/commands` (groups Workbench / Files / Plugin
commands); several shortcuts are hard-coded in `Workbench.svelte` and
`log.ts`; the keybindings backend uses different ids (`palette.open`,
`editor.save`, ...) from the frontend commands, so shortcuts changed in
Settings have no effect.

Decisions:

- **Menus in the title bar**, as VS Code does with its own title bar on
  Windows and Linux: File, Edit, View, Go, Build, Help, drawn by us in the
  top bar between the mark and the breadcrumb. Alt shows mnemonics, Alt+F
  opens File, F10 focuses the bar, arrow keys move, Esc closes. Below 900 px
  the menus fold into one "Menu" button. On macOS the same menus go to the
  native application menu instead (Tauri menu API), and the title bar keeps
  only the mark and breadcrumb.
- **Menus are built from the command registry**, never from separate
  handlers: each command gains an optional `menu: { bar, group, order }`.
  Missing actions become commands first: Open folder, Switch project,
  Undo/Redo/Cut/Copy/Paste/Find/Replace (dispatched to the focused editor,
  canvas or viewer), Show outline, Show source control, Keyboard shortcuts,
  About, Report a problem (opens the log folder), Quit.
- **One shortcut model:** the frontend handler reads the keybindings
  registry; backend and frontend use the same command ids; hard-coded
  shortcuts move into the registry; user changes in Settings take effect
  immediately; conflicts are shown.
- Menu items show their current shortcut, disabled state follows `when`
  clauses, and plugin commands can contribute menu items through their
  manifest (`[[contributes.menus]]`).

## 4. Contextual Settings

Today: Settings is one page; `settingsPage.show(section)` only scrolls to a
group; opening Settings replaces the editor context; settings carry `owner`
and `category` but no context.

Decisions:

- Settings opened from a place show **that place's settings only**, with the
  scope stated at the top and one link to show all:
  - from a document (gear in the editor, the Preview header, "Document
    settings" in the menu): Editor, Preview, Compile, the document's engine
    plugin (Typst, LaTeX or Markdown), Writing, Citations, plus that
    project's overrides;
  - from the figure editor: the Figure plugin's settings;
  - from the PDF viewer: Preview;
  - from Home or the activity bar: everything (application settings).
- The mapping is data, not code in the page: each settings group declares the
  contexts it belongs to (`contexts = ["document", "engine:typst"]` in the
  manifest or the core registry), and the UI context (`surface`, `editor`,
  `engine`) is kept while Settings is open instead of being replaced.
- The User / This project switch stays; in a document context it defaults to
  This project when the project overrides any shown setting.

## 5. Plugin manager

Today: plugins load from `<config>/manang/plugins/<name>/` only; the manager
can scan, sync, reload and unload, with a BLAKE3 change fingerprint; there is
no install, uninstall, enable/disable, consent, signature check or
`plugins.*` command; load failures are dropped at scan and `plugins.failed`
carries no detail; the Settings page lists plugins read-only.

Decisions:

- Backend: `plugins.list` (with state: loaded, disabled, failed with the
  error), `plugins.install` (from a folder or a `.manangplugin` archive:
  validated, copied, never executed before consent), `plugins.uninstall`,
  `plugins.enable` / `plugins.disable` (persisted per user, overridable per
  project), `plugins.reload`, `plugins.permissions` with grant and revoke
  persisted; events carry name and error text; scan failures are kept and
  shown.
- Consent: installing or updating shows the plugin's requested access in
  plain language (as the Settings page already phrases it) and asks once;
  grants are stored; a plugin whose new version asks for more is held until
  approved.
- Trust: ed25519 signatures (`plugin.sig`, ADR 0003 follow-up) verified
  against trusted publisher keys; unsigned plugins allowed by default for
  individuals, refused when policy says so (package 6).
- UI: a **Plugins** view in the activity bar (the fixed UI vocabulary already
  names it): installed list with state and errors, search, Install from
  file/folder, and a detail page (description, version, publisher, access,
  commands, settings link, enable/disable, uninstall, reload, logs).
  Built-in plugins appear as built in and cannot be uninstalled.

## 6. Enterprise policy layer

Today: no managed settings, no policy files, proxies are deliberately ignored
(`.proxy(None)`), no telemetry, no auto-update, English-only strings.

Decisions:

- **Managed policy file** read at start, below user and project in authority
  over nothing it does not set, and locking what it sets:
  `/etc/manang/policy.toml` (Linux), `%ProgramData%\Manang\policy.toml`
  (Windows), `/Library/Application Support/Manang/policy.toml` (macOS).
  Contents: locked setting values (shown in Settings as "Set by your
  organisation", not editable), plugin allowlist/blocklist and "signed only",
  network policy (allowed hosts, proxy), update channel, telemetry (always
  off unless policy enables an internal endpoint, which we do not ship).
- **Proxy support:** a `network.proxy` setting (system, none, or an explicit
  URL) honoured by citations and the WASM HTTP host; policy can set it.
- **Diagnostics without telemetry:** "Report a problem" collects the log
  folder, versions and settings (secrets excluded) into a zip the user can
  inspect and send themselves.
- Documented in `docs/admin.md` for IT administrators.

## 7. Release engineering (later)

Installers (MSI, deb, rpm, AppImage, dmg), code signing, an update channel
that respects policy, i18n (strings moved into message catalogues), an
accessibility audit with automated checks, and performance gates in CI
(launch time, idle memory, keystroke latency from AGENTS.md 4.2).
