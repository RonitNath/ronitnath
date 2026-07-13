import { fetchGuestbook, postGuestbookEntry } from "../lib/api";
import type { GuestbookEntry } from "../generated/GuestbookEntry";

function entryElement(entry: GuestbookEntry): HTMLLIElement {
  const item = document.createElement("li");
  item.className = "guestbook-entry";

  const author = document.createElement("p");
  author.className = "author";
  author.textContent = entry.author;

  const message = document.createElement("p");
  message.textContent = entry.message;

  const createdAt = document.createElement("p");
  createdAt.className = "muted created-at";
  createdAt.textContent = entry.created_at;

  item.append(author, message, createdAt);
  return item;
}

/** Mount the guestbook with the same form, loading, error, and append behavior
 * as the previous reactive island, using only browser DOM APIs. */
export function mountGuestbook(mount: HTMLElement): void {
  const form = document.createElement("form");
  form.className = "guestbook-form";

  const author = document.createElement("input");
  author.type = "text";
  author.placeholder = "Your name";
  author.required = true;
  author.maxLength = 500;

  const message = document.createElement("textarea");
  message.placeholder = "Leave a note";
  message.required = true;
  message.maxLength = 500;

  const submit = document.createElement("button");
  submit.type = "submit";
  submit.textContent = "Sign the guestbook";

  form.append(author, message, submit);

  const loading = document.createElement("p");
  loading.className = "muted";
  loading.textContent = "Loading entries…";
  mount.replaceChildren(form, loading);

  let list: HTMLUListElement | null = null;
  let error: HTMLParagraphElement | null = null;

  function setError(text: string | null): void {
    error?.remove();
    error = null;
    if (text === null) return;

    error = document.createElement("p");
    error.className = "guestbook-error";
    error.textContent = text;
    form.insertBefore(error, submit);
  }

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    setError(null);
    submit.disabled = true;
    submit.textContent = "Posting…";

    try {
      const created = await postGuestbookEntry({
        author: author.value,
        message: message.value,
      });
      list?.append(entryElement(created));
      author.value = "";
      message.value = "";
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Something went wrong.");
    } finally {
      submit.disabled = false;
      submit.textContent = "Sign the guestbook";
    }
  });

  void fetchGuestbook().then(
    (entries) => {
      list = document.createElement("ul");
      list.className = "guestbook-entries";
      list.append(...entries.map(entryElement));
      loading.replaceWith(list);
    },
    () => {
      const loadError = document.createElement("p");
      loadError.className = "guestbook-error";
      loadError.textContent = "Couldn't load entries.";
      loading.replaceWith(loadError);
    },
  );
}
