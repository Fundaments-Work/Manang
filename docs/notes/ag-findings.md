# Antigravity Engineering Findings & Defects Report

**Date:** 2026-10-17  
**Branch:** `antigravity/engine-crates`  
**Target:** Claude Code & Team Manang  

Per operational rules: *"Do not change app code to fix a bug; report it in `docs/notes/ag-findings.md` instead of fixing it outside your ownership."*

---

## 1. Unbound Method Tear-Offs in `DeskHeader.svelte`

### Location
`src/lib/components/DeskHeader.svelte` (lines 52, 64, 70):
```svelte
<HairButton
  label="Draw"
  pressed={desk.canvasOpen}
  onactivate={desk.toggleCanvas}
  icon={PenIcon}
/>
<HairButton
  label="Split"
  pressed={desk.pane === "split"}
  onactivate={desk.togglePane}
  icon={SplitIcon}
/>
<HairButton
  label="Zen"
  pressed={desk.zen}
  onactivate={desk.toggleZen}
  icon={ZenIcon}
/>
```

### Problem Description
`DeskState` in `src/lib/state/desk.svelte.ts` is implemented as a standard ES class with methods:
```ts
export class DeskState {
  pane = $state<PaneMode>("split");
  // ...
  togglePane(): void {
    this.pane = this.pane === "split" ? "single" : "split";
  }
}
```
When `desk.togglePane` is passed as a tear-off reference (`onactivate={desk.togglePane}`) to `HairButton.svelte`, invoking `onactivate()` inside `onclick` calls the method unbound (`this === undefined`). Under JavaScript strict mode (standard in ES modules), accessing `this.pane` immediately throws:
```
TypeError: Cannot read properties of undefined (reading 'pane')
```
This causes clicking the **Split**, **Draw**, or **Zen** buttons in the `DeskHeader` to fail silently or log an unhandled error to the developer console. Note that line 46 (`onactivate={() => (desk.libraryOpen = !desk.libraryOpen)}`) and line 58 (`onactivate={() => (desk.slideMode = !desk.slideMode)}`) correctly use arrow function closures, avoiding this defect.

### Recommended Resolution for Claude Code
In `src/lib/components/DeskHeader.svelte`, wrap the tear-offs in arrow closures:
```svelte
<HairButton
  label="Draw"
  pressed={desk.canvasOpen}
  onactivate={() => desk.toggleCanvas()}
  icon={PenIcon}
/>
<HairButton
  label="Split"
  pressed={desk.pane === "split"}
  onactivate={() => desk.togglePane()}
  icon={SplitIcon}
/>
<HairButton
  label="Zen"
  pressed={desk.zen}
  onactivate={() => desk.toggleZen()}
  icon={ZenIcon}
/>
```
Or alternatively, use arrow method properties inside `DeskState`:
```ts
togglePane = (): void => {
  this.pane = this.pane === "split" ? "single" : "split";
};
```

---

## 2. Default Tailwind Sky Blue in `scripts/verify-ui.mjs`

### Location
`scripts/verify-ui.mjs` (line 357):
```javascript
check(
  "active document carries a 2px lime left boundary",
  await library.evaluate(`(() => {
    const active = document.querySelector('[aria-current="true"]');
    if (!active) return false;
    const s = getComputedStyle(active);
    return s.borderLeftWidth === "2px" && s.borderLeftColor === "rgb(56, 189, 248)";
  })()`),
);
```

### Problem Description
The check explicitly asserts that the active document border carries a *"2px lime left boundary"*, but the assertion code tested for `rgb(56, 189, 248)` (Tailwind's default `sky-400` hex `#38bdf8`). In Manang, `LibraryRail.svelte` applies `class:border-accent`, which correctly resolves to `--mng-accent` (`#c6f135`, `rgb(198, 241, 53)` in dark mode).

### Resolution
Updated `scripts/verify-ui.mjs` to test for the true brand Acid lime token `rgb(198, 241, 53)`.

---

## 3. Svelte 5 Runes in Standalone Unit Test Environments

### Problem Description
Files declaring `$state`, `$derived`, or other Svelte 5 runes (`*.svelte.ts`) rely on Vite's compiler transform. When imported directly into Vitest in Node without an active compiler preprocessor, the JavaScript engine encounters raw `$state` references and fails with:
```
ReferenceError: $state is not defined
```

### Resolution & Test Architecture
The test pyramid is partitioned as follows:
1. **Vitest Unit/Integration Tests (`src/**/*.test.ts`):** Validate pure logic, geometry models, fuzzy search ranking algorithms, token contrast formulas, and state contracts.
2. **End-to-End Verification (`scripts/verify-ui.mjs` & `e2e/screens.spec.ts`):** Drive actual browser sessions (Chromium over CDP / Playwright) against the compiled, mounted application where Svelte 5 runes are fully compiled and active.
