import { onMount } from "solid-js";

/** Enhances the server-rendered admin forms and link controls. */
export default function EventsAdmin() {
  onMount(() => {
    for (const button of document.querySelectorAll<HTMLButtonElement>(".copy-btn")) {
      button.addEventListener("click", async () => {
        const text = button.dataset.copy;
        if (!text) return;
        try {
          await navigator.clipboard.writeText(text);
          const original = button.textContent;
          button.classList.add("copied");
          button.textContent = "Copied!";
          window.setTimeout(() => {
            button.classList.remove("copied");
            button.textContent = original;
          }, 1500);
        } catch {
          window.prompt("Copy this:", text);
        }
      });
    }

    for (const form of document.querySelectorAll<HTMLFormElement>(".photo-upload-form")) {
      form.addEventListener("submit", () => {
        const status = form.querySelector<HTMLElement>(".photo-upload-status");
        if (status) status.textContent = "Uploading…";
        form.querySelector<HTMLButtonElement>("button[type=submit]")?.setAttribute("disabled", "");
      });
    }
  });
  return null;
}
