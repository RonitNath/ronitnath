import { globalStyle } from "@vanilla-extract/css";
import { tokens } from "./theme.css";

/* ---------- Shared button classes ---------- */

globalStyle(".btn", {
  appearance: "none",
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  padding: "0.45rem 0.9rem",
  fontSize: "0.875rem",
  fontWeight: 600,
  cursor: "pointer",
  textDecoration: "none",
  display: "inline-flex",
  alignItems: "center",
  gap: "0.4rem",
  transition: "background 0.15s, color 0.15s, border-color 0.15s",
});

globalStyle(".btn-primary", {
  background: tokens.color.accent,
  borderColor: tokens.color.accent,
  color: tokens.color.accentFg,
});

globalStyle(".btn-primary:hover", {
  background: tokens.color.accentHover,
  borderColor: tokens.color.accentHover,
  color: tokens.color.accentFg,
  textDecoration: "none",
});

globalStyle(".btn-ghost", {
  background: "transparent",
  color: tokens.color.fgMuted,
});

globalStyle(".btn-ghost:hover", {
  color: tokens.color.fg,
  borderColor: tokens.color.fgSubtle,
  textDecoration: "none",
});

/* ---------- Events list header/toolbar ---------- */

globalStyle(".events-header", {
  display: "flex",
  alignItems: "flex-end",
  justifyContent: "space-between",
  gap: "1rem",
  flexWrap: "wrap",
  marginBottom: "1.5rem",
});

globalStyle(".admin-toolbar", {
  display: "flex",
  gap: "0.5rem",
  alignItems: "center",
});

/* ---------- Events list ---------- */

globalStyle(".events-list", {
  listStyle: "none",
  display: "flex",
  flexDirection: "column",
  gap: "0.75rem",
});

globalStyle(".event-card", {
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  background: tokens.color.bgMuted,
  transition: "border-color 0.15s, transform 0.15s",
});

globalStyle(".event-card:hover", {
  borderColor: tokens.color.accent,
});

globalStyle(".event-card-link", {
  display: "block",
  padding: "1rem 1.25rem",
  color: tokens.color.fg,
  textDecoration: "none",
});

globalStyle(".event-card-link:hover", {
  textDecoration: "none",
});

globalStyle(".event-card-head", {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: "0.75rem",
  flexWrap: "wrap",
});

globalStyle(".event-card-title", {
  fontFamily: tokens.font.display,
  fontSize: "1.5rem",
  fontWeight: "bold",
  color: tokens.color.fg,
  letterSpacing: "0.03em",
});

globalStyle(".event-card-status", {
  padding: "0.15rem 0.55rem",
  borderRadius: tokens.radius.base,
  background: tokens.color.bgSubtle,
  color: tokens.color.fgMuted,
  textTransform: "uppercase",
  fontSize: "0.7rem",
  fontWeight: 700,
  letterSpacing: "0.08em",
});

globalStyle('.event-card-status[data-status="published"]', {
  color: tokens.color.accent,
});

globalStyle('.event-card-status[data-status="archived"]', {
  color: tokens.color.fgSubtle,
});

globalStyle(".event-card-subtitle", {
  color: tokens.color.fgMuted,
  marginTop: "0.15rem",
});

globalStyle(".event-card-meta", {
  display: "flex",
  gap: "0.75rem",
  flexWrap: "wrap",
  color: tokens.color.fgSubtle,
  fontSize: "0.85rem",
  marginTop: "0.4rem",
});

globalStyle(".event-card-summary", {
  color: tokens.color.fgMuted,
  marginTop: "0.6rem",
  fontSize: "0.95rem",
});

/* ---------- Event detail ---------- */

globalStyle(".event-page", {
  minHeight: "calc(100vh - 57px)",
  padding: "3rem 1.5rem 4rem",
});

globalStyle(".event-content", {
  width: "100%",
  maxWidth: "760px",
  margin: "0 auto",
});

globalStyle(".event-title", {
  fontFamily: tokens.font.display,
  fontSize: "clamp(2.5rem, 7vw, 4rem)",
  fontWeight: "bold",
  color: tokens.color.fg,
  letterSpacing: "0.04em",
  lineHeight: 1,
  marginBottom: "0.5rem",
});

globalStyle(".event-subtitle", {
  color: tokens.color.fgMuted,
  fontSize: "1.125rem",
  marginBottom: "1.5rem",
});

globalStyle(".event-meta", {
  display: "flex",
  flexDirection: "column",
  gap: "0.4rem",
  padding: "0.75rem 1rem",
  border: `1px solid ${tokens.color.border}`,
  borderLeft: `4px solid ${tokens.color.accent}`,
  borderRadius: tokens.radius.base,
  background: tokens.color.bgMuted,
  marginBottom: "1.25rem",
});

globalStyle(".event-meta-row", {
  display: "flex",
  gap: "0.75rem",
  alignItems: "baseline",
  flexWrap: "wrap",
});

globalStyle(".event-meta-label", {
  color: tokens.color.fgMuted,
  textTransform: "uppercase",
  letterSpacing: "0.07em",
  fontSize: "0.7rem",
  fontWeight: 700,
  minWidth: "4.5rem",
});

globalStyle(".event-meta-value", {
  color: tokens.color.fg,
  fontSize: "0.95rem",
});

globalStyle(".event-summary", {
  color: tokens.color.fgMuted,
  fontSize: "1.05rem",
  lineHeight: 1.6,
  marginBottom: "1.25rem",
});

globalStyle(".event-details", {
  marginBottom: "1.5rem",
});

globalStyle(".event-details-pre", {
  whiteSpace: "pre-wrap",
  wordWrap: "break-word",
  fontFamily: "inherit",
  color: tokens.color.fg,
  background: tokens.color.bgMuted,
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  padding: "1rem",
  fontSize: "0.95rem",
  lineHeight: 1.55,
});

globalStyle(".event-section-heading", {
  fontFamily: tokens.font.display,
  fontSize: "1.25rem",
  letterSpacing: "0.05em",
  color: tokens.color.fg,
  marginTop: "1.5rem",
  marginBottom: "0.75rem",
  textTransform: "uppercase",
});

globalStyle(".event-schedule-list", {
  listStyle: "none",
  display: "flex",
  flexDirection: "column",
  gap: "0.5rem",
});

globalStyle(".event-schedule-item", {
  padding: "0.6rem 0.75rem",
  border: `1px solid ${tokens.color.border}`,
  borderRadius: tokens.radius.base,
  background: tokens.color.bgMuted,
});

globalStyle(".event-schedule-head", {
  display: "flex",
  justifyContent: "space-between",
  gap: "0.75rem",
  alignItems: "baseline",
  flexWrap: "wrap",
});

globalStyle(".event-schedule-time", {
  color: tokens.color.fgSubtle,
  fontSize: "0.85rem",
});

globalStyle(".event-schedule-details", {
  color: tokens.color.fgMuted,
  marginTop: "0.25rem",
});

globalStyle(".event-schedule-loc", {
  color: tokens.color.fgSubtle,
  fontSize: "0.85rem",
  marginTop: "0.15rem",
});

globalStyle(".event-actions", {
  display: "flex",
  gap: "0.75rem",
  flexWrap: "wrap",
  marginTop: "1rem",
});

/* ---------- Admin panel ---------- */

globalStyle(".admin-panel", {
  marginTop: "2rem",
  paddingTop: "1.5rem",
  borderTop: `1px dashed ${tokens.color.border}`,
});

globalStyle(".admin-hint", {
  color: tokens.color.fgMuted,
  fontSize: "0.9rem",
  marginBottom: "1rem",
});

globalStyle(".admin-grid [data-slot]:empty", {
  display: "none",
});

/* ---------- RSVP / signup pages ---------- */

globalStyle(".rsvp-page", {
  minHeight: "calc(100vh - 57px)",
  padding: "3rem 1.5rem 4rem",
});

globalStyle(".rsvp-content", {
  width: "100%",
  maxWidth: "620px",
  margin: "0 auto",
});

globalStyle(".rsvp-title", {
  fontFamily: tokens.font.display,
  fontSize: "clamp(2rem, 6vw, 3.25rem)",
  fontWeight: "bold",
  letterSpacing: "0.04em",
  color: tokens.color.fg,
  marginBottom: "0.4rem",
  lineHeight: 1,
});

globalStyle(".rsvp-greeting", {
  color: tokens.color.fgMuted,
  marginBottom: "1.25rem",
});
