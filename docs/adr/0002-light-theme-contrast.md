# ADR-0002: Light Theme WCAG Contrast Calibration

**Status:** Accepted  
**Date:** 2026-10-17  
**Deciders:** Fundaments.work  
**Affects:** `src/app.css` (`.theme-light`), `DESIGN.md` §2.2, automated token contrast tests

---

## 1. Context

Manang's canonical visual identity is set on a dark carbon base (`#141416`), where the signature **Acid lime** (`#c6f135`) achieves an exceptional contrast ratio of **14.16:1**.

In the light theme (the paper workspace override), surfaces are composed of:
- Page / Background: `--mng-bg` (`#faf9f7`)
- Surface: `--mng-surface` (`#f4f3f0`)
- Surface High: `--mng-surface-hi` (`#ffffff`)
- Surface Muted: `--mng-surface-muted` (`#e8e7e2`)
- Recessed: `--mng-recessed` (`#f4f3f0`)

Unadjusted Acid lime (`#c6f135`) against paper white achieves only **1.24:1**, completely failing WCAG requirements. Per the Fundaments.work design system rules, introducing an arbitrary new hue is strictly forbidden. Therefore, the light theme derives its accent token by darkening the lime via a linear sRGB mix toward the brand ink (`#1c1c1c`):
```css
--mng-accent: color-mix(in srgb, #c6f135 X%, #1c1c1c);
```

The initial draft specification used a **40% lime / 60% ink** ratio (`#607126`). A rigorous WCAG 2.1 contrast audit was conducted to verify compliance across all light surfaces.

---

## 2. Contrast Audit & Quantitative Findings

WCAG 2.1 defines two relevant success criteria:
- **SC 1.4.3 Contrast (Minimum) — Level AA:** Visual presentation of text and images of text requires a contrast ratio of at least **4.5:1** (3:1 for large text $\ge 18\text{pt}$ or $\ge 14\text{pt}$ bold).
- **SC 1.4.11 Non-text Contrast — Level AA:** Visual information required to identify user interface components (active controls, focus indicators, selection frames) requires at least **3.0:1**.

Because Manang uses `--mng-accent` for active selection indicators, active tabs, buttons (`aria-pressed:text-accent`), command highlights, and CodeMirror editor syntax tokens, the accent token must satisfy the higher text threshold (**$\ge 4.5:1$**) across all supported surfaces.

### 2.1. Audit of the 40% Lime Mix (`#607126`)

| Surface | Surface Hex | Accent Contrast | Target (UI: 3:1) | Target (Text: 4.5:1) | Result |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Surface-Hi** | `#ffffff` | **5.40:1** | Pass | Pass | Compliant |
| **Background** | `#faf9f7` | **5.13:1** | Pass | Pass | Compliant |
| **Surface** | `#f4f3f0` | **4.87:1** | Pass | Pass | Compliant |
| **Surface-Muted** | `#e8e7e2` | **4.36:1** | Pass | **FAIL (< 4.5:1)** | **Defect** |

**Finding:** At 40% lime, the accent token drops to **4.36:1** on `--mng-surface-muted`, failing WCAG AA for normal text by 0.14:1.

### 2.2. Audit of the Calibrated 35% Lime Mix (`#586725`)

Calibrating the mix ratio down to **35% lime / 65% ink** (`#586725`) darkens the token while strictly preserving its olive-lime chromatic temperature:

| Surface | Surface Hex | Accent Contrast | Target (UI: 3:1) | Target (Text: 4.5:1) | Result |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Surface-Hi** | `#ffffff` | **6.21:1** | Pass | Pass | Compliant ($\ge 4.5:1$) |
| **Background** | `#faf9f7` | **5.90:1** | Pass | Pass | Compliant ($\ge 4.5:1$) |
| **Surface** | `#f4f3f0` | **5.60:1** | Pass | Pass | Compliant ($\ge 4.5:1$) |
| **Surface-Muted** | `#e8e7e2` | **5.02:1** | Pass | Pass | Compliant ($\ge 4.5:1$) |
| **Accent-Ink (`#faf9f7`) on Accent** | `#586725` | **5.90:1** | Pass | Pass | Compliant ($\ge 4.5:1$) |

### 2.3. Audit of Text Tokens in Light Theme

| Token | Hex | Surface | Contrast Ratio | WCAG AA Status |
| :--- | :--- | :--- | :--- | :--- |
| `--mng-text` (Primary) | `#1c1c1c` | `#ffffff` | **17.04:1** | Pass (Exceeds AAA) |
| `--mng-text` (Primary) | `#1c1c1c` | `#faf9f7` | **16.20:1** | Pass (Exceeds AAA) |
| `--mng-text` (Primary) | `#1c1c1c` | `#f4f3f0` | **15.36:1** | Pass (Exceeds AAA) |
| `--mng-text` (Primary) | `#1c1c1c` | `#e8e7e2` | **13.76:1** | Pass (Exceeds AAA) |
| `--mng-text-60` (Secondary) | `#6b6b6b` | `#ffffff` | **5.33:1** | Pass (Exceeds AA) |
| `--mng-text-60` (Secondary) | `#6b6b6b` | `#faf9f7` | **5.06:1** | Pass (Exceeds AA) |
| `--mng-text-60` (Secondary) | `#6b6b6b` | `#f4f3f0` | **4.80:1** | Pass (Exceeds AA) |

---

## 3. Decision

1. **Adjust `.theme-light` `--mng-accent` mix ratio to 35% lime / 65% ink:**
   ```css
   --mng-accent: color-mix(in srgb, #c6f135 35%, #1c1c1c);
   ```
2. **Preserve strict design system invariants:** No new hex codes are introduced; the hue remains derived solely from Acid lime (`#c6f135`) and ink (`#1c1c1c`).
3. **Automate regression testing:** Add an automated Vitest test suite (`src/lib/theme/contrast.test.ts`) that asserts WCAG 2.1 contrast formulas on all light theme surfaces for both UI component boundaries ($\ge 3.0:1$) and text elements ($\ge 4.5:1$).
