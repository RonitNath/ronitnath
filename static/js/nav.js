// Mobile navigation drawer: the hamburger button toggles the off-canvas
// `.menu` drawer and its backdrop. No-ops on desktop, where the drawer styles
// don't apply.
(function () {
  "use strict";

  const toggle = document.querySelector(".nav-toggle");
  const menu = document.getElementById("menu");
  const overlay = document.getElementById("drawer-overlay");
  if (!toggle || !menu || !overlay) return;

  function setOpen(open) {
    menu.classList.toggle("open", open);
    overlay.classList.toggle("open", open);
    overlay.hidden = !open;
    toggle.setAttribute("aria-expanded", String(open));
  }

  toggle.addEventListener("click", function () {
    setOpen(!menu.classList.contains("open"));
  });

  overlay.addEventListener("click", function () {
    setOpen(false);
  });

  // Close after following a link, and on Escape.
  menu.querySelectorAll("a").forEach(function (link) {
    link.addEventListener("click", function () {
      setOpen(false);
    });
  });

  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") setOpen(false);
  });
})();
