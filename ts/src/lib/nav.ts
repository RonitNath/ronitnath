// Mobile navigation drawer: the hamburger button toggles the off-canvas
// `.menu` drawer and its backdrop. No-ops on desktop, where the drawer styles
// don't apply.
export function initNav(): void {
  const toggle = document.querySelector<HTMLButtonElement>(".nav-toggle");
  const menu = document.getElementById("menu");
  const overlay = document.getElementById("drawer-overlay");
  if (!toggle || !menu || !overlay) return;

  function setOpen(open: boolean): void {
    menu!.classList.toggle("open", open);
    overlay!.classList.toggle("open", open);
    (overlay as HTMLElement).hidden = !open;
    toggle!.setAttribute("aria-expanded", String(open));
  }

  toggle.addEventListener("click", () => {
    setOpen(!menu.classList.contains("open"));
  });

  overlay.addEventListener("click", () => setOpen(false));

  menu.querySelectorAll("a").forEach((link) => {
    link.addEventListener("click", () => setOpen(false));
  });

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") setOpen(false);
  });
}
