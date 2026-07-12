import { For, Show, createMemo, createResource, createSignal } from "solid-js";
import { fetchGuestView, postRsvp } from "../lib/events_api";
import type { SegmentChoice } from "../generated/SegmentChoice";

const PERSONAL_LINK_KEY = "gather-personal-link";

/** The guest RSVP island: overall yes/maybe/no, party size, a note, and a
 *  per-segment in/maybe/out row for each RSVP-able block of the day. */
export default function EventRsvp(props: { token: string }) {
  const [view] = createResource(() => props.token, fetchGuestView);

  const [status, setStatus] = createSignal<string | null>(null);
  const [name, setName] = createSignal("");
  const [partySize, setPartySize] = createSignal(1);
  const [note, setNote] = createSignal("");
  const [segments, setSegments] = createSignal<Record<number, string>>({});
  const [submitting, setSubmitting] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [savedName, setSavedName] = createSignal<string | null>(null);
  const [personalUrl, setPersonalUrl] = createSignal<string | null>(null);
  const [hydrated, setHydrated] = createSignal(false);

  const personalized = createMemo(() => view()?.person != null);

  const rsvpable = createMemo(() =>
    (view()?.schedule ?? []).filter((item) => item.segment_key !== null),
  );

  // Pre-fill from an existing RSVP on personalized links (once).
  createMemo(() => {
    const v = view();
    if (!v || hydrated()) return;
    if (v.person) {
      setName(v.person.name);
      if (v.person.attendance) {
        setStatus(v.person.attendance.status);
        setPartySize(v.person.attendance.party_size);
        setNote(v.person.attendance.note);
      }
      const seeded: Record<number, string> = {};
      for (const s of v.person.segments) seeded[s.schedule_item_id] = s.status;
      setSegments(seeded);
    }
    setHydrated(true);
  });

  function countFor(itemId: number): string {
    const c = view()?.segment_counts.find((c) => c.schedule_item_id === itemId);
    if (!c) return "";
    return `${c.in_count} in`;
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!status()) {
      setError("Pick going, maybe, or can't make it first.");
      return;
    }
    setError(null);
    setSubmitting(true);
    try {
      const chosen: SegmentChoice[] = Object.entries(segments()).map(
        ([id, s]) => ({ schedule_item_id: Number(id), status: s }),
      );
      const result = await postRsvp(props.token, {
        name: personalized() ? null : name(),
        status: status()!,
        party_size: partySize(),
        note: note(),
        segments: chosen,
      });
      setSavedName(result.person_name);
      if (result.personal_url) {
        setPersonalUrl(result.personal_url);
        try {
          localStorage.setItem(PERSONAL_LINK_KEY, result.personal_url);
        } catch {
          /* private mode etc. — the link is still shown on screen */
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Something went wrong.");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Show when={!view.loading} fallback={<p class="muted">Loading…</p>}>
      <Show
        when={!view.error}
        fallback={<p class="rsvp-error">Couldn't load the RSVP form — refresh to retry.</p>}
      >
        <form class="rsvp-form" onSubmit={handleSubmit}>
          <Show when={personalized()}>
            <p>
              Hi <strong>{view()!.person!.name}</strong>! If any plans change,
              please update this, it'll be a huge help for my planning. Thanks!
            </p>
          </Show>
          <Show when={!personalized()}>
            <label>
              Your name
              <input
                type="text"
                value={name()}
                onInput={(e) => setName(e.currentTarget.value)}
                required
                maxlength={100}
              />
            </label>
          </Show>

          <div class="rsvp-choices">
            <For each={["going", "maybe", "no"]}>
              {(s) => (
                <button
                  type="button"
                  classList={{ selected: status() === s }}
                  onClick={() => setStatus(s)}
                >
                  {s === "going" ? "I'm in" : s === "maybe" ? "Maybe" : "Can't make it"}
                </button>
              )}
            </For>
          </div>

          <Show when={status() !== "no" && rsvpable().length > 0}>
            <div>
              <For each={rsvpable()}>
                {(item) => (
                  <div class="segment-row">
                    <span class="segment-name">
                      {item.time_label} · {item.title}
                    </span>
                    <span class="segment-count">{countFor(item.id)}</span>
                    <For each={["in", "maybe", "out"]}>
                      {(s) => (
                        <button
                          type="button"
                          classList={{ selected: segments()[item.id] === s }}
                          onClick={() =>
                            setSegments((prev) => ({ ...prev, [item.id]: s }))
                          }
                        >
                          {s}
                        </button>
                      )}
                    </For>
                  </div>
                )}
              </For>
            </div>
          </Show>

          <Show when={status() === "going"}>
            <label>
              Bringing anyone? Total heads including you:
              <input
                type="number"
                min="1"
                max="10"
                value={partySize()}
                onInput={(e) => setPartySize(Number(e.currentTarget.value) || 1)}
              />
            </label>
          </Show>

          <label>
            Notes
            <textarea
              placeholder="Dietary needs, arrival time, or questions"
              value={note()}
              onInput={(e) => setNote(e.currentTarget.value)}
              maxlength={500}
            />
          </label>

          <Show when={error()}>
            <p class="rsvp-error">{error()}</p>
          </Show>

          <button class="rsvp-submit" type="submit" disabled={submitting()}>
            {submitting() ? "Saving…" : savedName() ? "Update my status" : "Save my status"}
          </button>

          <Show when={savedName()}>
            <div class="rsvp-saved">
              <p>
                Saved — thanks, {savedName()}!
                {status() === "going" ? " See you there." : ""}
              </p>
              <Show when={personalUrl()}>
                <p>
                  Save this personal link — it's your page for updating plans later:
                  <br />
                  <a class="rsvp-personal-link" href={personalUrl()!}>
                    {personalUrl()}
                  </a>
                </p>
              </Show>
            </div>
          </Show>
        </form>
      </Show>
    </Show>
  );
}
