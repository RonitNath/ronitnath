import { For, Show, createResource, createSignal, onMount } from "solid-js";

import type { GuestView } from "../generated/GuestView";
import type { SegmentChoice } from "../generated/SegmentChoice";
import { fetchGuestView, postRsvp } from "../lib/events_api";

const RSVP_STATUSES = ["going", "maybe", "no"] as const;
const SEGMENT_STATUSES = ["in", "maybe", "out"] as const;
const PERSONAL_LINK_KEY = "gather-personal-link";

export default function EventRsvp(props: { endpoint: string }) {
  const [view] = createResource(() => props.endpoint, fetchGuestView);
  const [status, setStatus] = createSignal<string | null>(null);
  const [partySize, setPartySize] = createSignal(1);
  const [name, setName] = createSignal("");
  const [note, setNote] = createSignal("");
  const [segments, setSegments] = createSignal<Record<number, string>>({});
  const [error, setError] = createSignal<string>();
  const [saving, setSaving] = createSignal(false);
  const [savedName, setSavedName] = createSignal<string>();
  const [personalUrl, setPersonalUrl] = createSignal<string>();

  onMount(() => {
    const dialog = document.querySelector<HTMLDialogElement>(".photo-dialog");
    for (const button of document.querySelectorAll<HTMLButtonElement>(".photo-open")) {
      button.addEventListener("click", () => {
        const image = dialog?.querySelector<HTMLImageElement>("img");
        const caption = dialog?.querySelector<HTMLElement>(".photo-dialog-caption");
        if (!dialog || !image || !caption || !button.dataset.photoSrc) return;
        image.src = button.dataset.photoSrc;
        image.alt = button.dataset.photoCaption || "Event photo";
        caption.textContent = button.dataset.photoCaption || "";
        dialog.showModal();
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

  function initialise(next: GuestView): void {
    if (status() !== null) return;
    setStatus(next.person?.attendance?.status ?? null);
    setPartySize(next.person?.attendance?.party_size ?? 1);
    setNote(next.person?.attendance?.note ?? "");
    setSegments(
      Object.fromEntries(
        (next.person?.segments ?? []).map((segment) => [segment.schedule_item_id, segment.status]),
      ),
    );
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const current = view();
    const selected = status();
    if (!current || !selected) {
      setError("Pick going, maybe, or can't make it first.");
      return;
    }
    setSaving(true);
    setError(undefined);
    try {
      const chosen: SegmentChoice[] = Object.entries(segments()).map(([id, choice]) => ({
        schedule_item_id: Number(id),
        status: choice,
      }));
      const result = await postRsvp(props.endpoint, {
        name: current.person ? null : name(),
        status: selected,
        party_size: partySize(),
        note: note(),
        segments: chosen,
      });
      setSavedName(result.person_name);
      setPersonalUrl(result.personal_url ?? undefined);
      if (result.personal_url) {
        try {
          localStorage.setItem(PERSONAL_LINK_KEY, result.personal_url);
        } catch {
          // A private browser can reject storage; the link remains on screen.
        }
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Something went wrong.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <Show when={view()} keyed fallback={<p class="muted">Loading reservation form…</p>}>
      {(current) => {
        initialise(current);
        const rsvpable = () => current.schedule.filter((item) => item.segment_key !== null);
        const segmentCount = (id: number) => current.segment_counts.find((item) => item.schedule_item_id === id)?.in_count;
        return (
          <form class="rsvp-form" onSubmit={submit}>
            <Show when={current.person}>
              {(person) => <p>Hi <strong>{person().name}</strong>! If any plans change, please update this, it'll be a huge help for my planning. Thanks!</p>}
            </Show>
            <Show when={!current.person}>
              <label>Your name<input type="text" required maxlength="100" value={name()} onInput={(event) => setName(event.currentTarget.value)} /></label>
            </Show>
            <div class="rsvp-choices">
              <For each={RSVP_STATUSES}>{(choice) => <button type="button" classList={{ selected: status() === choice }} onClick={() => setStatus(choice)}>{choice === "going" ? "I'm in" : choice === "maybe" ? "Maybe" : "Can't make it"}</button>}</For>
            </div>
            <Show when={status() !== "no" && rsvpable().length > 0}>
              <div>
                <For each={rsvpable()}>{(item) => <div class="segment-row">
                  <span class="segment-name">{item.time_label} · {item.title}</span>
                  <span class="segment-count">{segmentCount(item.id) ? `${segmentCount(item.id)} in` : ""}</span>
                  <For each={SEGMENT_STATUSES}>{(choice) => <button type="button" classList={{ selected: segments()[item.id] === choice }} onClick={() => setSegments({ ...segments(), [item.id]: choice })}>{choice}</button>}</For>
                </div>}</For>
              </div>
            </Show>
            <Show when={status() === "going"}>
              <label>Bringing anyone? Total heads including you:<input type="number" min="1" max="10" value={partySize()} onInput={(event) => setPartySize(Math.max(1, Number(event.currentTarget.value) || 1))} /></label>
            </Show>
            <label>Notes<textarea placeholder="Dietary needs, arrival time, or questions" maxlength="500" value={note()} onInput={(event) => setNote(event.currentTarget.value)} /></label>
            <Show when={error()}>{(message) => <p class="rsvp-error" role="alert">{message()}</p>}</Show>
            <button class="rsvp-submit" type="submit" disabled={saving()}>{saving() ? "Saving…" : savedName() ? "Update my status" : "Save my status"}</button>
            <Show when={savedName()}>{(guest) => <div class="rsvp-saved"><p>Saved — thanks, {guest()}!{status() === "going" ? " See you there." : ""}</p><Show when={personalUrl()}>{(url) => <p>Save this personal link — it's your page for updating plans later:<br /><a class="rsvp-personal-link" href={url()}>{url()}</a></p>}</Show></div>}</Show>
          </form>
        );
      }}
    </Show>
  );
}
