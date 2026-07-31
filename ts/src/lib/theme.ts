// Theme toggle: flips data-theme on <html> and persists the explicit
// choice. With nothing saved, base.css falls back to prefers-color-scheme,
// defaulting to dark when the system has no preference either.
export function initTheme(): void {
  const toggle = document.getElementById("theme-toggle");
  if (!toggle) return;

  function effectiveTheme(): "light" | "dark" {
    const explicit = document.documentElement.getAttribute("data-theme");
    if (explicit === "light" || explicit === "dark") return explicit;
    return matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  }

  toggle.addEventListener("click", () => {
    const next = effectiveTheme() === "dark" ? "light" : "dark";
    document.documentElement.setAttribute("data-theme", next);
    try {
      localStorage.setItem("theme", next);
    } catch {
      // localStorage may be unavailable (private mode); the toggle still
      // works for the rest of the session.
    }
  });
}
