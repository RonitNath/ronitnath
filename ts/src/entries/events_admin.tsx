// Plain-DOM helpers for the admin event page (no island needed): the
// copy-link / copy-invite buttons in the links table.

for (const button of document.querySelectorAll<HTMLButtonElement>(".copy-btn")) {
  button.addEventListener("click", async () => {
    const text = button.dataset.copy;
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      const original = button.textContent;
      button.classList.add("copied");
      button.textContent = "Copied!";
      setTimeout(() => {
        button.classList.remove("copied");
        button.textContent = original;
      }, 1500);
    } catch {
      // Clipboard API denied (http:// origin etc.) — fall back to a prompt
      // so the value is still reachable.
      window.prompt("Copy this:", text);
    }
  });
}
