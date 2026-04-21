import { globalFontFace, globalStyle } from "@vanilla-extract/css";
import { tokens } from "./theme.css";

/* ---------- Font ---------- */

globalFontFace("Agency Bold", {
  src: "url('/static/fonts/agency-bold.ttf') format('truetype')",
  fontWeight: "bold",
  fontDisplay: "swap",
});

/* ---------- Reset ---------- */

globalStyle("*, *::before, *::after", {
  boxSizing: "border-box",
  margin: 0,
  padding: 0,
});

globalStyle("html", {
  scrollbarGutter: "stable both-edges",
});

globalStyle("html, body", {
  minHeight: "100vh",
  backgroundColor: tokens.color.bg,
  color: tokens.color.fg,
  fontFamily: tokens.font.body,
  fontSize: "16px",
  lineHeight: 1.5,
});

globalStyle("a", {
  color: tokens.color.accent,
  textDecoration: "none",
});

globalStyle("a:hover", {
  color: tokens.color.accentHover,
  textDecoration: "underline",
});

