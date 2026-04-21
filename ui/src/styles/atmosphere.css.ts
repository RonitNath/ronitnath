import { globalKeyframes, globalStyle } from "@vanilla-extract/css";

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
