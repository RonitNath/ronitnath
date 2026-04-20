import { style } from "@vanilla-extract/css";
import { tokens } from "./theme.css";

export const toggle = style({
  background: "transparent",
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  color: tokens.color.fgMuted,
  cursor: "pointer",
  padding: "0.25rem 0.6rem",
  fontSize: "1rem",
  lineHeight: 1,
  transition: "color 0.15s, border-color 0.15s",
  selectors: {
    "&:hover": {
      color: tokens.color.fg,
      borderColor: tokens.color.fgSubtle,
    },
  },
});
