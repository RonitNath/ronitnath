import { For, Show, createMemo, createSignal } from "solid-js";
import * as styles from "@/styles/forms.css";

type Bootstrap = {
  event_ref: string;
  signup_token: string | null;
  event: {
    notes_label: string;
    notes_caption: string | null;
    dietary_label: string;
    arrival_note_label: string;
    arrival_note_caption: string | null;
  };
};

type GuestInput = {
  display_name: string;
  attending: boolean;
  dietary_restrictions: string;
  general_notes: string;
};

export function SignupForm(props: { bootstrap?: unknown }) {
  const boot = props.bootstrap as Bootstrap;
  const [displayName, setDisplayName] = createSignal("");
  const [email, setEmail] = createSignal("");
  const [phone, setPhone] = createSignal("");
  const [arrivalNote, setArrivalNote] = createSignal("");
  const [dietary, setDietary] = createSignal("");
  const [notes, setNotes] = createSignal("");
  const [guests, setGuests] = createSignal<GuestInput[]>([]);
  const [saving, setSaving] = createSignal(false);
  const [message, setMessage] = createSignal<{ kind: "ok" | "err"; text: string } | null>(null);
  const namedGuests = createMemo(() => guests().filter((g) => g.display_name.trim()));
  const attendingCount = createMemo(
    () => 1 + namedGuests().filter((g) => g.attending).length,
  );

  const addGuest = () => {
    setGuests((list) => [
      ...list,
      { display_name: "", attending: true, dietary_restrictions: "", general_notes: "" },
    ]);
  };
  const removeGuest = (idx: number) => setGuests((list) => list.filter((_, i) => i !== idx));
  const updateGuest = (idx: number, patch: Partial<GuestInput>) =>
    setGuests((list) => list.map((g, i) => (i === idx ? { ...g, ...patch } : g)));

  const submit = async (ev: SubmitEvent) => {
    ev.preventDefault();
    setMessage(null);
    if (!displayName().trim()) {
      setMessage({ kind: "err", text: "Please enter your name." });
      return;
    }
    setSaving(true);
    try {
      const payload = {
        display_name: displayName().trim(),
        email: email().trim() || null,
        phone: phone().trim() || null,
        rsvp: {
          rsvp_status: "yes",
          arrival_note: arrivalNote(),
          dietary_restrictions: dietary(),
          general_notes: notes(),
          guests: namedGuests().map((g) => ({
            id: null,
            display_name: g.display_name.trim(),
            attending: g.attending,
            dietary_restrictions: g.dietary_restrictions,
            general_notes: g.general_notes,
          })),
        },
      };
      const qs = boot.signup_token ? `?t=${encodeURIComponent(boot.signup_token)}` : "";
      const res = await fetch(
        `/events/${encodeURIComponent(boot.event_ref)}/signup${qs}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
        },
      );
      if (!res.ok) {
        const txt = await res.text();
        setMessage({ kind: "err", text: txt || `Error ${res.status}` });
      } else {
        const data = (await res.json()) as {
          rsvp_url: string;
          invitee: { location_approved: boolean };
        };
        setMessage({
          kind: "ok",
          text: !data.invitee.location_approved
              ? "You're on the waitlist. Redirecting to your RSVP..."
              : "You're signed up! Redirecting to your RSVP...",
        });
        window.setTimeout(() => {
          window.location.href = new URL(data.rsvp_url).pathname + new URL(data.rsvp_url).search;
        }, 600);
      }
    } catch (err) {
      setMessage({ kind: "err", text: `Network error: ${String(err)}` });
    } finally {
      setSaving(false);
    }
  };

  return (
    <form class={styles.form} onSubmit={submit} data-test="signup-form">
      <label class={styles.field}>
        <span class={styles.label}>Your name</span>
        <input
          class={styles.input}
          type="text"
          value={displayName()}
          onInput={(e) => setDisplayName(e.currentTarget.value)}
          required
          data-test="signup-name"
        />
      </label>
      <label class={styles.field}>
        <span class={styles.label}>Email (optional)</span>
        <input
          class={styles.input}
          type="email"
          value={email()}
          onInput={(e) => setEmail(e.currentTarget.value)}
          data-test="signup-email"
        />
      </label>
      <label class={styles.field}>
        <span class={styles.label}>Phone (optional)</span>
        <input
          class={styles.input}
          type="tel"
          value={phone()}
          onInput={(e) => setPhone(e.currentTarget.value)}
        />
      </label>
      <label class={styles.field}>
        <span class={styles.label}>{boot.event.arrival_note_label} (optional)</span>
        <Show when={boot.event.arrival_note_caption}>
          <span class={styles.caption}>{boot.event.arrival_note_caption}</span>
        </Show>
        <textarea
          class={styles.textarea}
          rows="2"
          maxLength="500"
          value={arrivalNote()}
          onInput={(e) => setArrivalNote(e.currentTarget.value)}
        />
      </label>
      <label class={styles.field}>
        <span class={styles.label}>{boot.event.dietary_label} (optional)</span>
        <textarea
          class={styles.textarea}
          rows="2"
          maxLength="1000"
          value={dietary()}
          onInput={(e) => setDietary(e.currentTarget.value)}
        />
      </label>
      <label class={styles.field}>
        <span class={styles.label}>{boot.event.notes_label} (optional)</span>
        <Show when={boot.event.notes_caption}>
          <span class={styles.caption}>{boot.event.notes_caption}</span>
        </Show>
        <textarea
          class={styles.textarea}
          rows="3"
          maxLength="4000"
          value={notes()}
          onInput={(e) => setNotes(e.currentTarget.value)}
        />
      </label>

      <fieldset class={styles.fieldset}>
        <legend class={styles.legend}>Guests (optional, {attendingCount()} going)</legend>
        <For each={guests()}>
          {(g, i) => (
            <div class={styles.guestCard}>
              <div class={styles.guestHead}>
                <input
                  class={styles.input}
                  type="text"
                  placeholder="Guest name (optional)"
                  value={g.display_name}
                  onInput={(e) => updateGuest(i(), { display_name: e.currentTarget.value })}
                />
                <label class={styles.inlineToggle}>
                  <input
                    type="checkbox"
                    checked={g.attending}
                    onChange={(e) => updateGuest(i(), { attending: e.currentTarget.checked })}
                  />
                  <span>Attending</span>
                </label>
                <button
                  type="button"
                  class={styles.btnGhost}
                  onClick={() => removeGuest(i())}
                >
                  Remove
                </button>
              </div>
            </div>
          )}
        </For>
        <button type="button" class={styles.btnGhost} onClick={addGuest}>
          Add guest
        </button>
      </fieldset>

      <div class={styles.actions}>
        <button
          type="submit"
          class={styles.btnPrimary}
          disabled={saving()}
          data-test="signup-submit"
        >
          {saving() ? "Signing up…" : "Sign up"}
        </button>
        <Show when={message()}>
          {(m) => (
            <span
              class={m().kind === "ok" ? styles.msgOk : styles.msgErr}
              role="status"
              data-test="signup-message"
            >
              {m().text}
            </span>
          )}
        </Show>
      </div>
    </form>
  );
}
