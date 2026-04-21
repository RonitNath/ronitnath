import { globalStyle } from "@vanilla-extract/css";
import { tokens } from "./theme.css";

/* ---------- Home page ---------- */

globalStyle(".home-card-wrapper", {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  minHeight: "calc(100vh - 57px)",
  padding: "2rem",
});

globalStyle(".home-card", {
  textAlign: "center",
  maxWidth: "500px",
});

globalStyle(".home-name", {
  fontFamily: tokens.font.display,
  fontSize: "clamp(2.5rem, 8vw, 5rem)",
  fontWeight: "bold",
  color: tokens.color.fg,
  letterSpacing: "0.05em",
  lineHeight: 1,
  marginBottom: "0.5rem",
});

globalStyle(".home-tagline", {
  fontSize: "1.125rem",
  color: tokens.color.fgMuted,
  marginBottom: "2rem",
});

globalStyle(".home-links", {
  display: "flex",
  gap: "1rem",
  justifyContent: "center",
  flexWrap: "wrap",
});

globalStyle(".home-link", {
  padding: "0.375rem 0.875rem",
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  color: tokens.color.fgMuted,
  fontSize: "0.875rem",
  textDecoration: "none",
  transition: "color 0.15s, border-color 0.15s",
});

globalStyle(".home-link:hover", {
  color: tokens.color.accent,
  borderColor: tokens.color.accent,
  textDecoration: "none",
});

/* ---------- Events page ---------- */

globalStyle(".events-page", {
  minHeight: "calc(100vh - 57px)",
  display: "flex",
  alignItems: "center",
  padding: "4rem 1.5rem",
  "@media": {
    "screen and (max-width: 640px)": {
      alignItems: "flex-start",
      paddingTop: "5rem",
    },
  },
});

globalStyle(".events-content", {
  width: "100%",
  maxWidth: "760px",
  margin: "0 auto",
});

globalStyle(".events-kicker", {
  color: tokens.color.accent,
  fontSize: "0.875rem",
  fontWeight: 700,
  letterSpacing: "0.08em",
  textTransform: "uppercase",
  marginBottom: "0.75rem",
});

globalStyle(".events-title", {
  fontFamily: tokens.font.display,
  fontSize: "4.5rem",
  fontWeight: "bold",
  color: tokens.color.fg,
  letterSpacing: "0.05em",
  lineHeight: 1,
  marginBottom: "2rem",
  "@media": {
    "screen and (max-width: 640px)": {
      fontSize: "3rem",
    },
  },
});

globalStyle(".events-banner", {
  display: "flex",
  gap: "1rem",
  alignItems: "center",
  justifyContent: "space-between",
  border: `1px solid ${tokens.color.border}`,
  borderLeft: `4px solid ${tokens.color.accent}`,
  borderRadius: tokens.radius.base,
  background: tokens.color.bgMuted,
  padding: "1rem 1.25rem",
  "@media": {
    "screen and (max-width: 640px)": {
      alignItems: "flex-start",
      flexDirection: "column",
    },
  },
});

globalStyle(".events-banner-label", {
  color: tokens.color.fg,
  fontWeight: 700,
  textTransform: "uppercase",
  whiteSpace: "nowrap",
});

globalStyle(".events-banner-text", {
  color: tokens.color.fgMuted,
  textAlign: "right",
  "@media": {
    "screen and (max-width: 640px)": {
      textAlign: "left",
    },
  },
});
