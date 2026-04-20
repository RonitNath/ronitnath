import { createGlobalTheme, globalStyle } from "@vanilla-extract/css";

export const tokens = createGlobalTheme(":root", {
  color: {
    bg: "oklch(0.06 0.005 240)",
    bgMuted: "oklch(0.10 0.008 240)",
    bgSubtle: "oklch(0.14 0.010 240)",
    fg: "oklch(0.96 0.002 80)",
    fgMuted: "oklch(0.75 0.005 80)",
    fgSubtle: "oklch(0.55 0.005 80)",
    border: "oklch(0.22 0.010 240)",
    accent: "oklch(0.65 0.15 210)",
    accentHover: "oklch(0.72 0.15 210)",
    accentFg: "oklch(0.06 0.005 240)",
  },
  font: {
    display: "'Agency Bold', system-ui, sans-serif",
    body: "system-ui, -apple-system, sans-serif",
  },
  radius: {
    base: "0.25rem",
  },
});

globalStyle(':root[data-theme="light"]', {
  vars: {
    [tokens.color.bg]: "oklch(0.97 0.003 240)",
    [tokens.color.bgMuted]: "oklch(0.93 0.005 240)",
    [tokens.color.bgSubtle]: "oklch(0.89 0.007 240)",
    [tokens.color.fg]: "oklch(0.15 0.010 240)",
    [tokens.color.fgMuted]: "oklch(0.35 0.008 240)",
    [tokens.color.fgSubtle]: "oklch(0.50 0.008 240)",
    [tokens.color.border]: "oklch(0.78 0.015 240)",
    [tokens.color.accent]: "oklch(0.45 0.15 210)",
    [tokens.color.accentHover]: "oklch(0.38 0.15 210)",
    [tokens.color.accentFg]: "oklch(0.97 0.003 240)",
  },
});
