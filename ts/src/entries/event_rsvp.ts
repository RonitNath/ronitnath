import { mountEventRsvp } from "../islands/EventRsvp";

const mount = document.getElementById("event-rsvp-island");
const endpoint = mount?.dataset.endpoint;
if (mount && endpoint) {
  mountEventRsvp(mount, endpoint);
}

const dialog = document.querySelector<HTMLDialogElement>(".photo-dialog");
document.querySelectorAll<HTMLButtonElement>(".photo-open").forEach((button) => {
  button.addEventListener("click", () => {
    const image = dialog?.querySelector<HTMLImageElement>("img");
    const caption = dialog?.querySelector<HTMLElement>(".photo-dialog-caption");
    if (!dialog || !image || !caption || !button.dataset.photoSrc) return;
    image.src = button.dataset.photoSrc;
    image.alt = button.dataset.photoCaption || "Event photo";
    caption.textContent = button.dataset.photoCaption || "";
    dialog.showModal();
  });
});

document.querySelectorAll<HTMLFormElement>(".photo-upload-form").forEach((form) => {
  form.addEventListener("submit", () => {
    const status = form.querySelector<HTMLElement>(".photo-upload-status");
    if (status) status.textContent = "Uploading…";
    form.querySelector<HTMLButtonElement>("button[type=submit]")?.setAttribute("disabled", "");
  });
});
