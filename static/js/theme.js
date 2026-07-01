// Theme toggle: flips data-theme on <html> and persists the explicit
// choice. With nothing saved, base.css falls back to prefers-color-scheme,
// defaulting to dark when the system has no preference either.
(function () {
  "use strict";

  const toggle = document.getElementById("theme-toggle");
  if (!toggle) return;

  function effectiveTheme() {
    const explicit = document.documentElement.getAttribute("data-theme");
    if (explicit) return explicit;
    return matchMedia("(prefers-color-scheme: light)").matches
      ? "light"
      : "dark";
  }

  toggle.addEventListener("click", function () {
    const next = effectiveTheme() === "dark" ? "light" : "dark";
    document.documentElement.setAttribute("data-theme", next);
    try {
      localStorage.setItem("theme", next);
    } catch (e) {}
  });
})();
