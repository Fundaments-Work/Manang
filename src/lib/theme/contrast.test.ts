/// <reference types="node" />

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

interface RGB {
  r: number;
  g: number;
  b: number;
}

/**
 * Parse a 6-character hex color "#RRGGBB" into RGB components [0, 255].
 */
function parseHex(hex: string): RGB {
  const clean = hex.trim().replace(/^#/, "");
  if (clean.length !== 6) {
    throw new Error(`Invalid hex string: ${hex}`);
  }
  const r = parseInt(clean.slice(0, 2), 16);
  const g = parseInt(clean.slice(2, 4), 16);
  const b = parseInt(clean.slice(4, 6), 16);
  if (Number.isNaN(r) || Number.isNaN(g) || Number.isNaN(b)) {
    throw new Error(`Failed to parse hex values: ${hex}`);
  }
  return { r, g, b };
}

/**
 * Mix two colors in linear sRGB space according to CSS `color-mix(in srgb, col1 p%, col2)`.
 */
function mixSrgb(col1: RGB, col2: RGB, weight1: number): RGB {
  const w1 = weight1 / 100;
  const w2 = 1 - w1;
  return {
    r: Math.round(col1.r * w1 + col2.r * w2),
    g: Math.round(col1.g * w1 + col2.g * w2),
    b: Math.round(col1.b * w1 + col2.b * w2),
  };
}

function toLinear(channel: number): number {
  const s = channel / 255;
  return s <= 0.04045 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
}

/**
 * Computes WCAG 2.1 relative luminance for an sRGB color.
 * Spec: https://www.w3.org/WAI/GL/wiki/Relative_luminance
 */
function relativeLuminance({ r, g, b }: RGB): number {
  return 0.2126 * toLinear(r) + 0.7152 * toLinear(g) + 0.0722 * toLinear(b);
}

/**
 * Computes WCAG 2.1 contrast ratio between two colors: (L1 + 0.05) / (L2 + 0.05).
 * Always returns a value >= 1.0.
 */
function contrastRatio(colorA: RGB, colorB: RGB): number {
  const lumA = relativeLuminance(colorA);
  const lumB = relativeLuminance(colorB);
  const l1 = Math.max(lumA, lumB);
  const l2 = Math.min(lumA, lumB);
  return (l1 + 0.05) / (l2 + 0.05);
}

describe("Light theme WCAG 2.1 contrast audit", () => {
  // Read and parse tokens from src/app.css directly from disk
  const cssUrl = new URL("../../app.css", import.meta.url);
  const appCss = readFileSync(cssUrl, "utf-8");

  const lightThemeBlock = appCss.match(/\.theme-light\s*\{([^}]+)\}/)?.[1];
  if (!lightThemeBlock) {
    throw new Error("Could not find .theme-light block in src/app.css");
  }

  function extractToken(varName: string): string {
    const regex = new RegExp(`${varName}:\\s*([^;]+);`);
    const match = lightThemeBlock?.match(regex);
    const tokenVal = match?.[1];
    if (!tokenVal) throw new Error(`Token ${varName} not found in .theme-light`);
    return tokenVal.trim();
  }

  const bgHex = extractToken("--mng-bg");
  const surfaceHex = extractToken("--mng-surface");
  const surfaceHiHex = extractToken("--mng-surface-hi");
  const surfaceMutedHex = extractToken("--mng-surface-muted");
  const textHex = extractToken("--mng-text");
  const text60Hex = extractToken("--mng-text-60");
  const accentInkHex = extractToken("--mng-accent-ink");
  const accentRaw = extractToken("--mng-accent");

  // Parse color-mix(in srgb, #c6f135 35%, #1c1c1c)
  const mixMatch = accentRaw.match(/color-mix\(in srgb,\s*(#[0-9a-fA-F]{6})\s+(\d+)%,\s*(#[0-9a-fA-F]{6})\)/);
  if (!mixMatch) {
    throw new Error(`Unexpected format for --mng-accent in .theme-light: ${accentRaw}`);
  }

  const color1Hex = mixMatch[1];
  const weightStr = mixMatch[2];
  const color2Hex = mixMatch[3];
  if (!color1Hex || !weightStr || !color2Hex) {
    throw new Error(`Failed to extract components from color-mix: ${accentRaw}`);
  }

  const mixWeight = parseInt(weightStr, 10);
  const accentRgb = mixSrgb(parseHex(color1Hex), parseHex(color2Hex), mixWeight);

  const surfaces = [
    { name: "--mng-surface-hi", rgb: parseHex(surfaceHiHex), hex: surfaceHiHex },
    { name: "--mng-bg", rgb: parseHex(bgHex), hex: bgHex },
    { name: "--mng-surface", rgb: parseHex(surfaceHex), hex: surfaceHex },
    { name: "--mng-surface-muted", rgb: parseHex(surfaceMutedHex), hex: surfaceMutedHex },
  ];

  it("ensures primary text (--mng-text) exceeds WCAG AAA (7:1) across all light surfaces", () => {
    const textRgb = parseHex(textHex);
    for (const surface of surfaces) {
      const ratio = contrastRatio(textRgb, surface.rgb);
      expect(ratio).toBeGreaterThanOrEqual(7.0);
    }
  });

  it("ensures secondary text (--mng-text-60) exceeds WCAG AA (4.5:1) on primary surfaces", () => {
    const text60Rgb = parseHex(text60Hex);
    for (const surface of surfaces.filter((s) => s.name !== "--mng-surface-muted")) {
      const ratio = contrastRatio(text60Rgb, surface.rgb);
      expect(ratio).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("ensures light theme accent satisfies WCAG SC 1.4.11 UI component contrast (>= 3.0:1) on all light surfaces", () => {
    for (const surface of surfaces) {
      const ratio = contrastRatio(accentRgb, surface.rgb);
      expect(ratio).toBeGreaterThanOrEqual(3.0);
    }
  });

  it("ensures calibrated light theme accent satisfies WCAG SC 1.4.3 text contrast (>= 4.5:1) on all light surfaces", () => {
    for (const surface of surfaces) {
      const ratio = contrastRatio(accentRgb, surface.rgb);
      expect(ratio).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("ensures accent ink (--mng-accent-ink) on accent background satisfies WCAG AA text contrast (>= 4.5:1)", () => {
    const accentInkRgb = parseHex(accentInkHex);
    const ratio = contrastRatio(accentInkRgb, accentRgb);
    expect(ratio).toBeGreaterThanOrEqual(4.5);
  });
});
