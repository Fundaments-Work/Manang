# Antigravity Requests & Integration Notes

## Source Control Panel Integration
- **Panel component:** `src/lib/components/SourceControlPanel.svelte` (and `src/lib/components/panels/SourceControlPanel.svelte`).
- **Layout wiring:** Added `"source-control"` to `SideView` and `SIDE_VIEWS` with icon `"source-control"` in `src/lib/workbench/layout.ts`.
- **Icon:** Added `"source-control"` icon in `src/lib/components/ui/Icon.svelte`.
- **Workbench integration note:** When integrating `ActivityBar.svelte` and `SideBar.svelte`, wire `view === "source-control"` to render `<SourceControlPanel />`.
