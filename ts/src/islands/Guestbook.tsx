import { For, Show, createResource, createSignal } from "solid-js";
import { fetchGuestbook, postGuestbookEntry } from "../lib/api";

export default function Guestbook() {
  const [entries, { mutate }] = createResource(fetchGuestbook);
  const [author, setAuthor] = createSignal("");
  const [message, setMessage] = createSignal("");
  const [submitting, setSubmitting] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    setError(null);
    setSubmitting(true);
    try {
      const created = await postGuestbookEntry({ author: author(), message: message() });
      mutate((prev) => [...(prev ?? []), created]);
      setAuthor("");
      setMessage("");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Something went wrong.");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <>
      <form class="guestbook-form" onSubmit={handleSubmit}>
        <input
          type="text"
          placeholder="Your name"
          value={author()}
          onInput={(e) => setAuthor(e.currentTarget.value)}
          required
          maxlength={500}
        />
        <textarea
          placeholder="Leave a note"
          value={message()}
          onInput={(e) => setMessage(e.currentTarget.value)}
          required
          maxlength={500}
        />
        <Show when={error()}>
          <p class="guestbook-error">{error()}</p>
        </Show>
        <button type="submit" disabled={submitting()}>
          {submitting() ? "Posting…" : "Sign the guestbook"}
        </button>
      </form>

      <Show when={!entries.loading} fallback={<p class="muted">Loading entries…</p>}>
        <Show
          when={!entries.error}
          fallback={<p class="guestbook-error">Couldn't load entries.</p>}
        >
          <ul class="guestbook-entries">
            <For each={entries()}>
              {(entry) => (
                <li class="guestbook-entry">
                  <p class="author">{entry.author}</p>
                  <p>{entry.message}</p>
                  <p class="muted created-at">{entry.created_at}</p>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>
    </>
  );
}
