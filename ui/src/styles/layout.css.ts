import { globalStyle } from "@vanilla-extract/css";
import { tokens } from "./theme.css";

/* ---------- Nav ---------- */

globalStyle(".site-nav", {
  position: "relative",
  zIndex: 10,
  borderBottom: `1px solid ${tokens.color.border}`,
  background: "transparent",
});

globalStyle(".nav-inner", {
  maxWidth: "1200px",
  margin: "0 auto",
  padding: "0.75rem 1.5rem",
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
});

globalStyle(".nav-links", {
  display: "flex",
  gap: "1.5rem",
});

globalStyle(".nav-link", {
  color: tokens.color.fgMuted,
  fontSize: "0.875rem",
  letterSpacing: "0.05em",
  textTransform: "uppercase",
  textDecoration: "none",
  transition: "color 0.15s",
});

globalStyle(".nav-link:hover", {
  color: tokens.color.fg,
  textDecoration: "none",
});

globalStyle(".nav-mode", {
  display: "flex",
  alignItems: "center",
});

/* ---------- Main layout ---------- */

globalStyle(".site-main", {
  position: "relative",
  zIndex: 1,
  minHeight: "calc(100vh - 57px)",
});

