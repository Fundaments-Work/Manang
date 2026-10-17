# manang-settings: notes for kernel and shell owners

## Kernel change wanted: more than one command namespace per plugin

`Manifest::validate` requires every command to start with `<plugin-name>.`.
The requested surface has three namespaces (`settings.*`, `keybindings.*`,
`theme.*`) owned by one plugin. Workaround used here: three manifests
(`manifest()`, `keybindings_manifest()`, `theme_manifest()`, or
`manifests()`), all registered with the SAME `Arc<dyn Plugin>`. The plugin
answers all fourteen commands regardless of which manifest routed them.

Cleaner kernel option: an optional `namespaces = ["keybindings", "theme"]`
under `[plugin]` that widens the prefix check. The workaround then collapses
into the single `manifest()`.

## Behaviour worth knowing

* Command ids for default keybindings (`palette.open`, `library.toggle`,
  `zen.toggle`, `editor.save`, ...) are proposals. The shell's current palette
  ids (`open-library`, `toggle-zen`) are not dotted; align them or register
  your own defaults through `keybindings.register`.
* Files are re-read on external change using notify. The project layer watch
  covers `<project>/.manang/` and the project root (non-recursive); if the
  `.manang` folder is created externally after the project was selected,
  call `settings.use-project` again (or `settings.open-file {scope:"project"}`,
  which creates it and re-arms the watcher).
* The `themes/` folder is scanned on each `theme.list`/`theme.get`; it is not
  watched and emits no event. Selecting a user theme is the
  `appearance.custom-theme` setting.
* Settings commands do file I/O synchronously; invoke from a blocking task.
