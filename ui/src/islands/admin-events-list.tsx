import { Show, createSignal } from "solid-js";
import * as styles from "@/styles/forms.css";
import {
  rfc3339ToDateValue,
  rfc3339ToTimeValue,
  updateDatePart,
  updateTimePart,
} from "./datetime";

export function AdminEventsList() {
  const [open, setOpen] = createSignal(false);
  const [title, setTitle] = createSignal("");
  const [startsAt, setStartsAt] = createSignal("");
  const [endsAt, setEndsAt] = createSignal("");
  const [timezone, setTimezone] = createSignal("America/Los_Angeles");
  const [visibility, setVisibility] = createSignal<"public" | "unlisted" | "invite_only">(
    "invite_only",
  );
  const [signupMode, setSignupMode] = createSignal<"invite_only" | "self_signup">("invite_only");
  const [attendeeCap, setAttendeeCap] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const onClickCreate = () => {
    setOpen(true);
    setError(null);
  };

  const submit = async (ev: SubmitEvent) => {
    ev.preventDefault();
    setError(null);
    if (!title().trim() || !startsAt() || !endsAt()) {
      setError("Title, start, and end are required.");
      return;
    }
    setSaving(true);
    try {
      const cap = attendeeCap().trim();
      const payload = {
        slug: null,
        title: title().trim(),
        starts_at: startsAt(),
        ends_at: endsAt(),
        timezone: timezone(),
        visibility: visibility(),
        signup_mode: signupMode(),
        attendee_cap: cap ? Number(cap) : null,
        created_by_isoastra_identity_id: null,
      };
      const res = await fetch("/events", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      if (!res.ok) {
        setError(await res.text());
        return;
      }
      const data = (await res.json()) as { event: { id: string; slug: string | null } };
      const ref = data.event.slug ?? data.event.id;
      window.location.href = `/events/${encodeURIComponent(ref)}`;
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  // Wire the toolbar button
  if (typeof document !== "undefined") {
    queueMicrotask(() => {
      const btn = document.querySelector('[data-action="create"]');
      if (btn) btn.addEventListener("click", onClickCreate);
    });
  }

  return (
    <Show when={open()}>
      <div class={styles.modalBackdrop} data-test="new-event-modal">
        <div class={styles.modal}>
          <h3 class={styles.adminHeading}>New event</h3>
          <form class={styles.form} onSubmit={submit}>
            <label class={styles.field}>
              <span class={styles.label}>Title</span>
              <input
                class={styles.input}
                type="text"
                value={title()}
                onInput={(e) => setTitle(e.currentTarget.value)}
                required
                data-test="event-title-input"
              />
            </label>
            <label class={styles.field}>
              <span class={styles.label}>Starts at</span>
              <div class={styles.dateTimePair}>
                <input
                  class={styles.input}
                  type="date"
                  value={rfc3339ToDateValue(startsAt())}
                  onInput={(e) => setStartsAt(updateDatePart(startsAt(), e.currentTarget.value, "18:00"))}
                  data-test="event-starts-date"
                />
                <input
                  class={styles.input}
                  type="time"
                  value={rfc3339ToTimeValue(startsAt())}
                  onInput={(e) => setStartsAt(updateTimePart(startsAt(), e.currentTarget.value))}
                  data-test="event-starts-time"
                />
                <input
                  class={styles.input}
                  type="text"
                  placeholder="2026-05-01T18:00:00-07:00"
                  value={startsAt()}
                  onInput={(e) => setStartsAt(e.currentTarget.value)}
                  required
                  data-test="event-starts-input"
                />
              </div>
            </label>
            <label class={styles.field}>
              <span class={styles.label}>Ends at</span>
              <div class={styles.dateTimePair}>
                <input
                  class={styles.input}
                  type="date"
                  value={rfc3339ToDateValue(endsAt())}
                  onInput={(e) => setEndsAt(updateDatePart(endsAt(), e.currentTarget.value, "22:00"))}
                  data-test="event-ends-date"
                />
                <input
                  class={styles.input}
                  type="time"
                  value={rfc3339ToTimeValue(endsAt())}
                  onInput={(e) => setEndsAt(updateTimePart(endsAt(), e.currentTarget.value))}
                  data-test="event-ends-time"
                />
                <input
                  class={styles.input}
                  type="text"
                  placeholder="2026-05-01T22:00:00-07:00"
                  value={endsAt()}
                  onInput={(e) => setEndsAt(e.currentTarget.value)}
                  required
                  data-test="event-ends-input"
                />
              </div>
            </label>
            <label class={styles.field}>
              <span class={styles.label}>Timezone</span>
              <input
                class={styles.input}
                type="text"
                value={timezone()}
                onInput={(e) => setTimezone(e.currentTarget.value)}
                required
              />
            </label>
            <div class={styles.adminRow}>
              <label class={styles.field}>
                <span class={styles.label}>Visibility</span>
                <select
                  class={styles.input}
                  value={visibility()}
                  onChange={(e) =>
                    setVisibility(e.currentTarget.value as "public" | "unlisted" | "invite_only")
                  }
                  data-test="event-visibility-input"
                >
                  <option value="invite_only">Invite only</option>
                  <option value="unlisted">Unlisted</option>
                  <option value="public">Public</option>
                </select>
              </label>
              <label class={styles.field}>
                <span class={styles.label}>Signup mode</span>
                <select
                  class={styles.input}
                  value={signupMode()}
                  onChange={(e) =>
                    setSignupMode(e.currentTarget.value as "invite_only" | "self_signup")
                  }
                  data-test="event-signup-mode-input"
                >
                  <option value="invite_only">Invite only</option>
                  <option value="self_signup">Self signup</option>
                </select>
              </label>
              <label class={styles.field}>
                <span class={styles.label}>Attendee cap (optional)</span>
                <input
                  class={styles.input}
                  type="number"
                  min="1"
                  value={attendeeCap()}
                  onInput={(e) => setAttendeeCap(e.currentTarget.value)}
                />
              </label>
            </div>
            <Show when={error()}>
              <div class={styles.msgErr} role="alert">
                {error()}
              </div>
            </Show>
            <div class={styles.actions}>
              <button
                type="submit"
                class={styles.btnPrimary}
                disabled={saving()}
                data-test="create-event-submit"
              >
                {saving() ? "Creating…" : "Create"}
              </button>
              <button
                type="button"
                class={styles.btnGhost}
                onClick={() => setOpen(false)}
              >
                Cancel
              </button>
            </div>
          </form>
        </div>
      </div>
    </Show>
  );
}
