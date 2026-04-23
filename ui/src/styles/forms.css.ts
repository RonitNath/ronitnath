import { globalStyle, style } from "@vanilla-extract/css";
import { tokens } from "./theme.css";

export const form = style({
  display: "flex",
  flexDirection: "column",
  gap: "1rem",
  marginTop: "1.25rem",
});

export const fieldset = style({
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  padding: "0.75rem 1rem 1rem",
  display: "flex",
  flexDirection: "column",
  gap: "0.75rem",
  background: tokens.color.bgMuted,
});

export const legend = style({
  padding: "0 0.5rem",
  color: tokens.color.fg,
  fontWeight: 600,
  fontSize: "0.95rem",
});

export const radioRow = style({
  display: "grid",
  gridTemplateColumns: "repeat(3, minmax(0, 1fr))",
  gap: "0.5rem",
  maxWidth: "32rem",
  "@media": {
    "screen and (max-width: 520px)": {
      gridTemplateColumns: "1fr",
    },
  },
});

export const radioLabel = style({
  position: "relative",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  minHeight: "2.75rem",
  padding: "0.65rem 1rem",
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  cursor: "pointer",
  background: tokens.color.bgSubtle,
  color: tokens.color.fg,
  fontWeight: 700,
  transition: "background 0.15s, border-color 0.15s, color 0.15s, box-shadow 0.15s",
  selectors: {
    "&:hover": {
      borderColor: tokens.color.fgSubtle,
      color: tokens.color.fg,
    },
    "&:focus-within": {
      outline: "none",
      borderColor: tokens.color.accent,
      boxShadow: `0 0 0 1px ${tokens.color.accent}`,
    },
    "&:has(input:checked)": {
      borderColor: tokens.color.accent,
      background: `color-mix(in oklch, ${tokens.color.accent} 18%, ${tokens.color.bgSubtle})`,
      color: tokens.color.fg,
      boxShadow: `inset 0 0 0 1px ${tokens.color.accent}`,
    },
  },
});

globalStyle(`.${radioLabel} input`, {
  position: "absolute",
  inset: 0,
  opacity: 0,
  cursor: "pointer",
});

globalStyle(`.${radioLabel} span`, {
  position: "relative",
  zIndex: 1,
});

export const field = style({
  display: "flex",
  flexDirection: "column",
  gap: "0.35rem",
});

export const dateTimePair = style({
  display: "grid",
  gridTemplateColumns: "minmax(0, 1fr) minmax(0, 0.8fr)",
  gap: "0.6rem",
  "@media": {
    "screen and (max-width: 640px)": {
      gridTemplateColumns: "1fr",
    },
  },
});

globalStyle(`.${dateTimePair} > input[type='text']`, {
  gridColumn: "1 / -1",
});

export const label = style({
  color: tokens.color.fgMuted,
  fontSize: "0.875rem",
  fontWeight: 600,
});

export const caption = style({
  color: tokens.color.fgSubtle,
  fontSize: "0.8rem",
});

export const input = style({
  background: tokens.color.bgSubtle,
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  color: tokens.color.fg,
  font: "inherit",
  padding: "0.5rem 0.7rem",
  selectors: {
    "&:focus": {
      outline: "none",
      borderColor: tokens.color.accent,
    },
  },
});

export const textarea = style([
  input,
  {
    resize: "vertical",
    minHeight: "2.5rem",
  },
]);

export const actions = style({
  display: "flex",
  alignItems: "center",
  gap: "0.75rem",
  flexWrap: "wrap",
  marginTop: "0.25rem",
});

export const btnPrimary = style({
  appearance: "none",
  border: `1px solid ${tokens.color.accent}`,
  borderRadius: tokens.radius.base,
  background: tokens.color.accent,
  color: tokens.color.accentFg,
  padding: "0.5rem 1rem",
  fontWeight: 700,
  fontSize: "0.9rem",
  letterSpacing: "0.02em",
  cursor: "pointer",
  transition: "background 0.15s, border-color 0.15s",
  selectors: {
    "&:hover:not(:disabled)": {
      background: tokens.color.accentHover,
      borderColor: tokens.color.accentHover,
    },
    "&:disabled": {
      opacity: 0.6,
      cursor: "not-allowed",
    },
  },
});

export const btnGhost = style({
  appearance: "none",
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  background: "transparent",
  color: tokens.color.fgMuted,
  padding: "0.4rem 0.8rem",
  font: "inherit",
  fontSize: "0.875rem",
  cursor: "pointer",
  transition: "color 0.15s, border-color 0.15s",
  selectors: {
    "&:hover:not(:disabled)": {
      color: tokens.color.fg,
      borderColor: tokens.color.fgSubtle,
    },
    "&:disabled": {
      opacity: 0.5,
      cursor: "not-allowed",
    },
  },
});

export const msgOk = style({
  color: tokens.color.accent,
  fontWeight: 600,
  fontSize: "0.9rem",
});

export const msgErr = style({
  color: "oklch(0.65 0.18 25)",
  fontWeight: 600,
  fontSize: "0.9rem",
});

export const guestCard = style({
  display: "flex",
  flexDirection: "column",
  gap: "0.5rem",
  padding: "0.6rem 0.75rem",
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  background: tokens.color.bgSubtle,
});

export const guestHead = style({
  display: "flex",
  alignItems: "center",
  gap: "0.5rem",
  flexWrap: "wrap",
});

export const inlineToggle = style({
  display: "inline-flex",
  alignItems: "center",
  gap: "0.35rem",
  color: tokens.color.fgMuted,
  fontSize: "0.875rem",
});

export const adminPanel = style({
  display: "flex",
  flexDirection: "column",
  gap: "1.25rem",
  marginTop: "1rem",
});

export const adminRow = style({
  display: "grid",
  gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
  gap: "1rem",
});

export const adminSection = style({
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  padding: "1rem",
  background: tokens.color.bgMuted,
});

export const adminHeading = style({
  fontSize: "0.95rem",
  fontWeight: 700,
  color: tokens.color.fg,
  marginBottom: "0.5rem",
  textTransform: "uppercase",
  letterSpacing: "0.06em",
});

export const adminLine = style({
  color: tokens.color.fgMuted,
  fontSize: "0.9rem",
  marginBottom: "0.5rem",
});

export const adminButtons = style({
  display: "flex",
  gap: "0.5rem",
  flexWrap: "wrap",
});

export const adminWarn = style({
  color: "oklch(0.72 0.15 55)",
});

export const adminForm = style({
  display: "grid",
  gridTemplateColumns: "1fr 1fr auto auto",
  gap: "0.5rem",
  marginBottom: "0.75rem",
  "@media": {
    "screen and (max-width: 640px)": {
      gridTemplateColumns: "1fr",
    },
  },
});

export const adminTable = style({
  width: "100%",
  borderCollapse: "collapse",
  fontSize: "0.875rem",
});

globalStyle(`.${adminTable} th, .${adminTable} td`, {
  padding: "0.4rem 0.5rem",
  borderBottom: `1px solid ${tokens.color.border}`,
  textAlign: "left",
});

globalStyle(`.${adminTable} th`, {
  color: tokens.color.fgMuted,
  fontWeight: 600,
  textTransform: "uppercase",
  letterSpacing: "0.05em",
  fontSize: "0.75rem",
});

export const linkReveal = style({
  border: `1px dashed ${tokens.color.accent}`,
  borderRadius: tokens.radius.base,
  padding: "0.75rem",
  marginTop: "0.75rem",
  background: tokens.color.bgSubtle,
  display: "flex",
  flexDirection: "column",
  gap: "0.5rem",
});

export const linkCode = style({
  fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
  fontSize: "0.8rem",
  wordBreak: "break-all",
  color: tokens.color.fg,
  background: tokens.color.bg,
  padding: "0.4rem 0.5rem",
  borderRadius: tokens.radius.base,
  border: `1px solid ${tokens.color.border}`,
});

export const modalBackdrop = style({
  position: "fixed",
  inset: 0,
  background: "rgba(0, 0, 0, 0.55)",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  zIndex: 50,
  padding: "1rem",
});

export const modal = style({
  background: tokens.color.bgMuted,
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  padding: "1.5rem",
  width: "100%",
  maxWidth: "560px",
  maxHeight: "90vh",
  overflowY: "auto",
});
