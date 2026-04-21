import { globalFontFace, globalKeyframes, globalStyle } from "@vanilla-extract/css";
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

/* ---------- Starfield ---------- */

globalStyle(".starfield", {
  position: "fixed",
  inset: 0,
  pointerEvents: "none",
  zIndex: 0,
});

globalStyle(".starfield > div", {
  position: "absolute",
  inset: 0,
});

/* Dark mode stars — dense, bright, cool-white */
globalStyle('[data-theme="dark"] .stars-dim', {
  backgroundImage: [
    "radial-gradient(1px 1px at 23px 45px, rgba(255,255,255,0.5), transparent)",
    "radial-gradient(1px 1px at 89px 123px, rgba(200,210,255,0.45), transparent)",
    "radial-gradient(1px 1px at 156px 67px, rgba(255,255,255,0.4), transparent)",
    "radial-gradient(1px 1px at 234px 189px, rgba(180,200,255,0.45), transparent)",
    "radial-gradient(1px 1px at 45px 267px, rgba(200,220,255,0.42), transparent)",
    "radial-gradient(1px 1px at 178px 312px, rgba(255,255,255,0.48), transparent)",
    "radial-gradient(1px 1px at 267px 89px, rgba(180,200,255,0.4), transparent)",
    "radial-gradient(1px 1px at 312px 34px, rgba(255,255,255,0.38), transparent)",
    "radial-gradient(1px 1px at 56px 234px, rgba(200,210,255,0.45), transparent)",
    "radial-gradient(1px 1px at 123px 156px, rgba(255,255,255,0.42), transparent)",
    "radial-gradient(1px 1px at 198px 23px, rgba(220,230,255,0.4), transparent)",
    "radial-gradient(1px 1px at 67px 389px, rgba(255,255,255,0.35), transparent)",
    "radial-gradient(1px 1px at 345px 145px, rgba(200,215,255,0.42), transparent)",
    "radial-gradient(1px 1px at 278px 312px, rgba(255,255,255,0.38), transparent)",
    "radial-gradient(1px 1px at 134px 401px, rgba(180,200,255,0.4), transparent)",
  ].join(", "),
  backgroundSize: [
    "250px 230px",
    "283px 257px",
    "311px 213px",
    "267px 251px",
    "297px 227px",
    "259px 267px",
    "301px 203px",
    "321px 239px",
    "243px 273px",
    "289px 191px",
    "277px 221px",
    "303px 247px",
    "261px 209px",
    "293px 263px",
    "271px 237px",
  ].join(", "),
});

globalStyle('[data-theme="dark"] .stars-med', {
  animation: "twinkle-slow 15s ease-in-out infinite alternate",
  backgroundImage: [
    "radial-gradient(1.2px 1.2px at 67px 12px, rgba(255,255,255,0.65), transparent)",
    "radial-gradient(1.2px 1.2px at 145px 178px, rgba(200,210,255,0.6), transparent)",
    "radial-gradient(1.2px 1.2px at 223px 89px, rgba(255,255,255,0.7), transparent)",
    "radial-gradient(1.2px 1.2px at 312px 256px, rgba(180,200,255,0.58), transparent)",
    "radial-gradient(1.2px 1.2px at 89px 345px, rgba(200,220,255,0.62), transparent)",
    "radial-gradient(1.2px 1.2px at 178px 401px, rgba(255,255,255,0.55), transparent)",
    "radial-gradient(1.2px 1.2px at 289px 145px, rgba(180,200,255,0.65), transparent)",
    "radial-gradient(1.2px 1.2px at 401px 45px, rgba(255,255,255,0.6), transparent)",
    "radial-gradient(1.2px 1.2px at 34px 289px, rgba(220,230,255,0.58), transparent)",
    "radial-gradient(1.2px 1.2px at 367px 367px, rgba(255,255,255,0.55), transparent)",
    "radial-gradient(1.2px 1.2px at 445px 123px, rgba(200,210,255,0.6), transparent)",
    "radial-gradient(1.2px 1.2px at 156px 56px, rgba(255,255,255,0.52), transparent)",
  ].join(", "),
  backgroundSize: [
    "349px 299px",
    "387px 329px",
    "361px 273px",
    "403px 311px",
    "379px 287px",
    "343px 321px",
    "391px 267px",
    "367px 303px",
    "353px 313px",
    "399px 281px",
    "371px 297px",
    "341px 331px",
  ].join(", "),
});

globalStyle('[data-theme="dark"] .stars-bright', {
  animation: "twinkle-med 7s ease-in-out infinite alternate",
  backgroundImage: [
    "radial-gradient(1.8px 1.8px at 34px 89px, rgba(255,255,255,0.9), transparent)",
    "radial-gradient(1.8px 1.8px at 167px 234px, rgba(200,215,255,0.8), transparent)",
    "radial-gradient(1.8px 1.8px at 312px 45px, rgba(255,255,255,0.95), transparent)",
    "radial-gradient(1.8px 1.8px at 478px 178px, rgba(180,200,255,0.75), transparent)",
    "radial-gradient(1.8px 1.8px at 134px 345px, rgba(255,255,255,0.85), transparent)",
    "radial-gradient(1.8px 1.8px at 345px 267px, rgba(200,220,255,0.7), transparent)",
    "radial-gradient(2px 2px at 234px 456px, rgba(255,255,255,0.9), transparent)",
    "radial-gradient(1.8px 1.8px at 489px 312px, rgba(200,215,255,0.8), transparent)",
    "radial-gradient(2px 2px at 67px 489px, rgba(255,255,255,0.85), transparent)",
  ].join(", "),
  backgroundSize: [
    "457px 387px",
    "501px 419px",
    "471px 399px",
    "493px 423px",
    "463px 411px",
    "487px 381px",
    "509px 437px",
    "479px 403px",
    "451px 427px",
  ].join(", "),
});

/* Light mode stars — golden, warm tones */
globalStyle('[data-theme="light"] .stars-dim', {
  backgroundImage: [
    "radial-gradient(2.5px 2.5px at 23px 45px, rgba(210,160,0,0.6), transparent)",
    "radial-gradient(2.5px 2.5px at 89px 123px, rgba(210,160,0,0.55), transparent)",
    "radial-gradient(2.5px 2.5px at 156px 67px, rgba(210,160,0,0.5), transparent)",
    "radial-gradient(2.5px 2.5px at 234px 189px, rgba(210,160,0,0.55), transparent)",
    "radial-gradient(2.5px 2.5px at 45px 267px, rgba(210,160,0,0.52), transparent)",
    "radial-gradient(2.5px 2.5px at 178px 312px, rgba(210,160,0,0.58), transparent)",
    "radial-gradient(2.5px 2.5px at 267px 89px, rgba(210,160,0,0.5), transparent)",
    "radial-gradient(2.5px 2.5px at 312px 34px, rgba(210,160,0,0.48), transparent)",
    "radial-gradient(2.5px 2.5px at 56px 234px, rgba(210,160,0,0.55), transparent)",
    "radial-gradient(2.5px 2.5px at 123px 156px, rgba(210,160,0,0.5), transparent)",
    "radial-gradient(2.5px 2.5px at 198px 23px, rgba(210,160,0,0.55), transparent)",
    "radial-gradient(2.5px 2.5px at 345px 145px, rgba(210,160,0,0.52), transparent)",
  ].join(", "),
  backgroundSize: [
    "250px 230px",
    "283px 257px",
    "311px 213px",
    "267px 251px",
    "297px 227px",
    "259px 267px",
    "301px 203px",
    "321px 239px",
    "243px 273px",
    "289px 191px",
    "277px 221px",
    "261px 209px",
  ].join(", "),
});

globalStyle('[data-theme="light"] .stars-med', {
  animation: "twinkle-slow 15s ease-in-out infinite alternate",
  backgroundImage: [
    "radial-gradient(2.5px 2.5px at 67px 12px, rgba(210,160,0,0.7), transparent)",
    "radial-gradient(2.5px 2.5px at 145px 178px, rgba(210,160,0,0.65), transparent)",
    "radial-gradient(2.5px 2.5px at 223px 89px, rgba(210,160,0,0.7), transparent)",
    "radial-gradient(2.5px 2.5px at 312px 256px, rgba(210,160,0,0.6), transparent)",
    "radial-gradient(2.5px 2.5px at 89px 345px, rgba(210,160,0,0.68), transparent)",
    "radial-gradient(2.5px 2.5px at 178px 401px, rgba(210,160,0,0.6), transparent)",
    "radial-gradient(2.5px 2.5px at 289px 145px, rgba(210,160,0,0.65), transparent)",
    "radial-gradient(2.5px 2.5px at 401px 45px, rgba(210,160,0,0.62), transparent)",
  ].join(", "),
  backgroundSize: [
    "349px 299px",
    "387px 329px",
    "361px 273px",
    "403px 311px",
    "379px 287px",
    "343px 321px",
    "391px 267px",
    "367px 303px",
  ].join(", "),
});

globalStyle('[data-theme="light"] .stars-bright', {
  animation: "twinkle-med 7s ease-in-out infinite alternate",
  backgroundImage: [
    "radial-gradient(3px 3px at 34px 89px, rgba(210,160,0,0.85), transparent)",
    "radial-gradient(3px 3px at 167px 234px, rgba(210,160,0,0.75), transparent)",
    "radial-gradient(3px 3px at 312px 45px, rgba(210,160,0,0.8), transparent)",
    "radial-gradient(3px 3px at 478px 178px, rgba(210,160,0,0.7), transparent)",
    "radial-gradient(3px 3px at 134px 345px, rgba(210,160,0,0.75), transparent)",
    "radial-gradient(3px 3px at 345px 267px, rgba(210,160,0,0.65), transparent)",
  ].join(", "),
  backgroundSize: [
    "457px 387px",
    "501px 419px",
    "471px 399px",
    "493px 423px",
    "463px 411px",
    "487px 381px",
  ].join(", "),
});

globalKeyframes("twinkle-med", {
  "0%": { opacity: 0.15 },
  "40%": { opacity: 0.9 },
  "70%": { opacity: 0.3 },
  "100%": { opacity: 1 },
});

globalKeyframes("twinkle-slow", {
  "0%": { opacity: 0.7 },
  "50%": { opacity: 0.9 },
  "100%": { opacity: 1 },
});

/* ---------- Nebula ---------- */

globalStyle('[data-theme="dark"] .nebula', {
  position: "fixed",
  top: 0,
  left: 0,
  right: 0,
  bottom: 0,
  pointerEvents: "none",
  zIndex: 0,
  background: [
    "radial-gradient(ellipse 600px 400px at 15% 10%, rgba(60, 80, 180, 0.08), transparent)",
    "radial-gradient(ellipse 500px 500px at 85% 30%, rgba(100, 60, 160, 0.06), transparent)",
    "radial-gradient(ellipse 700px 300px at 50% 70%, rgba(40, 70, 150, 0.05), transparent)",
    "radial-gradient(ellipse 400px 600px at 90% 90%, rgba(80, 50, 140, 0.04), transparent)",
  ].join(", "),
});

globalStyle('[data-theme="light"] .nebula', {
  display: "none",
});

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
