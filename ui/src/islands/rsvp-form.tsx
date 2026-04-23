import { For, Show, createMemo, createSignal } from "solid-js";
import * as styles from "@/styles/forms.css";

type GuestInput = {
  id: string | null;
  display_name: string;
  attending: boolean;
  dietary_restrictions: string;
  general_notes: string;
};

type Status = "yes" | "no" | "maybe";

type Bootstrap = {
  event_ref: string;
  token: string;
  invitee: {
    display_name: string;
    party_size_limit: number;
    rsvp_status: string;
    arrival_note: string;
    dietary_restrictions: string;
    general_notes: string;
  };
  guests: Array<{
    id: string;
    display_name: string;
    attending: boolean;
    dietary_restrictions: string;
    general_notes: string;
  }>;
  event: {
    notes_label: string;
    notes_caption: string | null;
    dietary_label: string;
    arrival_note_label: string;
    arrival_note_caption: string | null;
  };
};

function initialStatus(raw: string): Status {
  return raw === "yes" || raw === "no" || raw === "maybe" ? raw : "yes";
}

export function RsvpForm(props: { bootstrap?: unknown }) {
  const boot = props.bootstrap as Bootstrap;
  const [status, setStatus] = createSignal<Status>(initialStatus(boot.invitee.rsvp_status));
  const [arrivalNote, setArrivalNote] = createSignal(boot.invitee.arrival_note);
  const [dietary, setDietary] = createSignal(boot.invitee.dietary_restrictions);
  const [notes, setNotes] = createSignal(boot.invitee.general_notes);
  const [guests, setGuests] = createSignal<GuestInput[]>(
    boot.guests.map((g) => ({
      id: g.id,
      display_name: g.display_name,
      attending: g.attending,
      dietary_restrictions: g.dietary_restrictions,
      general_notes: g.general_notes,
    })),
  );
  const [saving, setSaving] = createSignal(false);
  const [message, setMessage] = createSignal<{ kind: "ok" | "err"; text: string } | null>(null);

  const partyLimit = boot.invitee.party_size_limit;
  const attendingCount = createMemo(() => 1 + guests().filter((g) => g.attending).length);
  const canAddGuest = createMemo(() => guests().length + 1 < partyLimit);

  const addGuest = () => {
    if (!canAddGuest()) return;
    setGuests((list) => [
      ...list,
      {
        id: null,
        display_name: "",
        attending: true,
        dietary_restrictions: "",
        general_notes: "",
      },
    ]);
  };

  const removeGuest = (idx: number) => {
    setGuests((list) => list.filter((_, i) => i !== idx));
  };

  const updateGuest = (idx: number, patch: Partial<GuestInput>) => {
    setGuests((list) => list.map((g, i) => (i === idx ? { ...g, ...patch } : g)));
  };

  const submit = async (ev: SubmitEvent) => {
    ev.preventDefault();
    setMessage(null);
    if (status() === "yes" && attendingCount() > partyLimit) {
      setMessage({ kind: "err", text: `Party size exceeds limit of ${partyLimit}.` });
      return;
    }
    setSaving(true);
    try {
      const payload = {
        rsvp_status: status(),
        arrival_note: arrivalNote(),
        dietary_restrictions: dietary(),
        general_notes: notes(),
        guests: guests().map((g) => ({
          id: g.id,
          display_name: g.display_name,
          attending: g.attending,
          dietary_restrictions: g.dietary_restrictions,
          general_notes: g.general_notes,
        })),
      };
      const res = await fetch(
        `/events/${encodeURIComponent(boot.event_ref)}/r/${encodeURIComponent(boot.token)}`,
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
        setMessage({ kind: "ok", text: "Saved. Thank you!" });
      }
    } catch (err) {
      setMessage({ kind: "err", text: `Network error: ${String(err)}` });
    } finally {
      setSaving(false);
    }
  };

  return (
    <form class={styles.form} onSubmit={submit} data-test="rsvp-form">
      <fieldset class={styles.fieldset}>
        <legend class={styles.legend}>Are you coming?</legend>
        <div class={styles.radioRow}>
          <For each={["yes", "no", "maybe"] as Status[]}>
            {(s) => (
              <label class={styles.radioLabel} data-test={`rsvp-status-${s}`}>
                <input
                  type="radio"
                  name="rsvp_status"
                  value={s}
                  checked={status() === s}
                  onChange={() => setStatus(s)}
                />
                <span>{s === "yes" ? "Yes" : s === "no" ? "No" : "Maybe"}</span>
              </label>
            )}
          </For>
        </div>
      </fieldset>

      <Show when={status() !== "no"}>
        <label class={styles.field}>
          <span class={styles.label}>{boot.event.arrival_note_label}</span>
          <Show when={boot.event.arrival_note_caption}>
            <span class={styles.caption}>{boot.event.arrival_note_caption}</span>
          </Show>
          <textarea
            class={styles.textarea}
            rows="2"
            maxLength="500"
            value={arrivalNote()}
            onInput={(e) => setArrivalNote(e.currentTarget.value)}
            data-test="rsvp-arrival-note"
          />
        </label>

        <label class={styles.field}>
          <span class={styles.label}>{boot.event.dietary_label}</span>
          <textarea
            class={styles.textarea}
            rows="2"
            maxLength="1000"
            value={dietary()}
            onInput={(e) => setDietary(e.currentTarget.value)}
            data-test="rsvp-dietary"
          />
        </label>

        <label class={styles.field}>
          <span class={styles.label}>{boot.event.notes_label}</span>
          <Show when={boot.event.notes_caption}>
            <span class={styles.caption}>{boot.event.notes_caption}</span>
          </Show>
          <textarea
            class={styles.textarea}
            rows="3"
            maxLength="4000"
            value={notes()}
            onInput={(e) => setNotes(e.currentTarget.value)}
            data-test="rsvp-notes"
          />
        </label>

        <Show when={partyLimit > 1}>
          <fieldset class={styles.fieldset}>
            <legend class={styles.legend}>
              Guests ({attendingCount()} / {partyLimit} going including you)
            </legend>
            <For each={guests()}>
              {(g, i) => (
                <div class={styles.guestCard} data-test={`guest-row-${i()}`}>
                  <div class={styles.guestHead}>
                    <input
                      class={styles.input}
                      type="text"
                      placeholder="Guest name"
                      value={g.display_name}
                      onInput={(e) => updateGuest(i(), { display_name: e.currentTarget.value })}
                      data-test={`guest-name-${i()}`}
                      required
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
                      data-test={`guest-remove-${i()}`}
                    >
                      Remove
                    </button>
                  </div>
                  <input
                    class={styles.input}
                    type="text"
                    placeholder="Dietary notes"
                    value={g.dietary_restrictions}
                    onInput={(e) =>
                      updateGuest(i(), { dietary_restrictions: e.currentTarget.value })
                    }
                    maxLength="1000"
                  />
                  <input
                    class={styles.input}
                    type="text"
                    placeholder="Notes"
                    value={g.general_notes}
                    onInput={(e) => updateGuest(i(), { general_notes: e.currentTarget.value })}
                    maxLength="1000"
                  />
                </div>
              )}
            </For>
            <Show when={canAddGuest()}>
              <button
                type="button"
                class={styles.btnGhost}
                onClick={addGuest}
                data-test="guest-add"
              >
                Add guest
              </button>
            </Show>
          </fieldset>
        </Show>
      </Show>

      <div class={styles.actions}>
        <button
          type="submit"
          class={styles.btnPrimary}
          disabled={saving()}
          data-test="rsvp-submit"
        >
          {saving() ? "Saving…" : "Save RSVP"}
        </button>
        <Show when={message()}>
          {(m) => (
            <span
              class={m().kind === "ok" ? styles.msgOk : styles.msgErr}
              role="status"
              data-test="rsvp-message"
            >
              {m().text}
            </span>
          )}
        </Show>
      </div>
    </form>
  );
}
