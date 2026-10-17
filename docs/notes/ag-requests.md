# Antigravity Requests & Integration Notes

## Source Control Panel Integration
- **Panel component:** `src/lib/components/SourceControlPanel.svelte` (and `src/lib/components/panels/SourceControlPanel.svelte`).
- **Layout wiring:** Added `"source-control"` to `SideView` and `SIDE_VIEWS` with icon `"source-control"` in `src/lib/workbench/layout.ts`.
- **Icon:** Added `"source-control"` icon in `src/lib/components/ui/Icon.svelte`.
- **Workbench integration note:** When integrating `ActivityBar.svelte` and `SideBar.svelte`, wire `view === "source-control"` to render `<SourceControlPanel />`.

## Citations & References Panel Integration
- **References panel:** Rewired `src/lib/components/panels/ReferencesPanel.svelte` directly to `citationsApi(kernel)` (`citations.load`, `citations.search`, `citations.format`, `citations.verify`).
- **Workspace state deprecation note:** `src/lib/state/workspace.svelte.ts` still has `loadReferences()` calling `parseBibtex`. We maintained `parseBibtex`, `BibEntry`, and `authorSummary` backwards-compatibility in `src/lib/workbench/references.ts`. When workspace state is ready, `workspace.references` can be removed in favor of `citationsApi(kernel)`.
