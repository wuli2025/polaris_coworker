import { describe, expect, it } from "vitest";
import {
  NAS_DARK_THEME,
  NAS_LIGHT_THEME,
  nasThemeVars,
  type NasThemeVars,
} from "./nasTheme";

function channelToLinear(channel: number): number {
  const value = channel / 255;
  return value <= 0.04045
    ? value / 12.92
    : ((value + 0.055) / 1.055) ** 2.4;
}

function luminance(hex: string): number {
  const match = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex);
  if (!match) throw new Error(`expected six-digit hex color, got ${hex}`);
  const [, red, green, blue] = match;
  return (
    0.2126 * channelToLinear(Number.parseInt(red, 16)) +
    0.7152 * channelToLinear(Number.parseInt(green, 16)) +
    0.0722 * channelToLinear(Number.parseInt(blue, 16))
  );
}

function contrast(foreground: string, background: string): number {
  const high = Math.max(luminance(foreground), luminance(background));
  const low = Math.min(luminance(foreground), luminance(background));
  return (high + 0.05) / (low + 0.05);
}

function expectReadable(palette: NasThemeVars) {
  const pairs: Array<[keyof NasThemeVars, keyof NasThemeVars]> = [
    ["--nas-text", "--nas-panel"],
    ["--nas-secondary", "--nas-form"],
    ["--nas-text", "--nas-control"],
    ["--nas-placeholder", "--nas-control"],
    ["--nas-muted", "--nas-card"],
    ["--nas-secondary", "--nas-card-hover"],
  ];
  for (const [foreground, background] of pairs) {
    expect(
      contrast(palette[foreground], palette[background]),
      `${foreground} must remain readable on ${background}`,
    ).toBeGreaterThanOrEqual(4.5);
  }
}

function expectStateAndActionReadable(palette: NasThemeVars) {
  for (const background of ["--nas-primary-start", "--nas-primary-end"] as const) {
    expect(
      contrast(palette["--nas-primary-label"], palette[background]),
      `primary label must remain readable on ${background}`,
    ).toBeGreaterThanOrEqual(4.5);
  }
  expect(
    contrast(palette["--nas-ok"], palette["--nas-ok-soft"]),
    "connected state must remain readable",
  ).toBeGreaterThanOrEqual(4.5);
}

describe("NAS manager theme", () => {
  it("keeps all light-theme dialog text readable", () => {
    expectReadable(NAS_LIGHT_THEME);
    expectStateAndActionReadable(NAS_LIGHT_THEME);
  });

  it("keeps all dark-theme dialog text readable", () => {
    expectReadable(NAS_DARK_THEME);
    expectStateAndActionReadable(NAS_DARK_THEME);
  });

  it("routes both aurora variants to the matching contrast palette", () => {
    expect(nasThemeVars("aurora-light")).toBe(NAS_LIGHT_THEME);
    expect(nasThemeVars("light")).toBe(NAS_LIGHT_THEME);
    expect(nasThemeVars("aurora-dark")).toBe(NAS_DARK_THEME);
    expect(nasThemeVars("dark")).toBe(NAS_DARK_THEME);
  });
});
