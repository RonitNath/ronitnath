import { For, Show, createSignal, onMount } from "solid-js";
import * as styles from "@/styles/forms.css";
import {
  rfc3339ToDateValue,
  rfc3339ToTimeValue,
  updateDatePart,
  updateTimePart,
} from "./datetime";

type InviteeRow = {
  id: string;
  display_name: string;
  email: string | null;
  rsvp_status: string;
  party_size_limit: number;
  location_approved: boolean;
  opened_at: string | null;
  responded_at: string | null;
};

type Bootstrap = {
  event_ref: string;
  event_id: string;
  confirmed: number;
  over_cap: number;
  public_confirmed: number;
  cap: number | null;
  title: string;
  subtitle: string | null;
  summary: string | null;
  details_markdown: string;
  approximate_location_name: string | null;
  location_name: string | null;
  address: string | null;
  map_url: string | null;
  display_capacity: boolean;
  self_signup_requires_approval: boolean;
  notes_label: string;
  notes_caption: string | null;
  dietary_label: string;
  arrival_note_label: string;
  arrival_note_caption: string | null;
  allow_rsvp_edits: boolean;
  status: string;
  visibility: string;
  signup_mode: string;
  starts_at: string;
  ends_at: string;
  timezone: string;
  attendee_cap: number | null;
  self_signup_token_set: boolean;
  invitees: InviteeRow[];
};

async function postJson<T>(url: string, body?: unknown): Promise<T> {
  const init: RequestInit = {
    method: "POST",
  };
  if (body !== undefined) {
    init.headers = { "Content-Type": "application/json" };
    init.body = JSON.stringify(body);
  }
  const res = await fetch(url, init);
  if (!res.ok) throw new Error(await res.text());
  return (await res.json()) as T;
}

const nullable = (value: string) => {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
};

export function AdminEventControls(props: { bootstrap?: unknown }) {
  const boot = props.bootstrap as Bootstrap;
  const [title, setTitle] = createSignal(boot.title);
  const [subtitle, setSubtitle] = createSignal(boot.subtitle ?? "");
  const [summary, setSummary] = createSignal(boot.summary ?? "");
  const [details, setDetails] = createSignal(boot.details_markdown);
  const [approximateLocation, setApproximateLocation] = createSignal(
    boot.approximate_location_name ?? "",
  );
  const [locationName, setLocationName] = createSignal(boot.location_name ?? "");
  const [address, setAddress] = createSignal(boot.address ?? "");
  const [mapUrl, setMapUrl] = createSignal(boot.map_url ?? "");
  const [displayCapacity, setDisplayCapacity] = createSignal(boot.display_capacity);
  const [requiresApproval, setRequiresApproval] = createSignal(
    boot.self_signup_requires_approval,
  );
  const [notesLabel, setNotesLabel] = createSignal(boot.notes_label);
  const [notesCaption, setNotesCaption] = createSignal(boot.notes_caption ?? "");
  const [dietaryLabel, setDietaryLabel] = createSignal(boot.dietary_label);
  const [arrivalNoteLabel, setArrivalNoteLabel] = createSignal(boot.arrival_note_label);
  const [arrivalNoteCaption, setArrivalNoteCaption] = createSignal(
    boot.arrival_note_caption ?? "",
  );
  const [allowRsvpEdits, setAllowRsvpEdits] = createSignal(boot.allow_rsvp_edits);
  const [status, setStatus] = createSignal(boot.status);
  const [visibility, setVisibility] = createSignal(boot.visibility);
  const [signupMode, setSignupMode] = createSignal(boot.signup_mode);
  const [startsAt, setStartsAt] = createSignal(boot.starts_at);
  const [endsAt, setEndsAt] = createSignal(boot.ends_at);
  const [timezone, setTimezone] = createSignal(boot.timezone);
  const [attendeeCap, setAttendeeCap] = createSignal(
    boot.attendee_cap === null ? "" : String(boot.attendee_cap),
  );
  const [confirmed, setConfirmed] = createSignal(boot.confirmed);
  const [overCap, setOverCap] = createSignal(boot.over_cap);
  const [publicConfirmed, setPublicConfirmed] = createSignal(boot.public_confirmed);
  const [invitees, setInvitees] = createSignal<InviteeRow[]>(boot.invitees);
  const [newName, setNewName] = createSignal("");
  const [newEmail, setNewEmail] = createSignal("");
  const [newParty, setNewParty] = createSignal(1);
  const [createdLink, setCreatedLink] = createSignal<{ name: string; url: string } | null>(null);
  const [signupUrl, setSignupUrl] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  const refresh = async () => {
    try {
      const [invRes, capRes] = await Promise.all([
        fetch(`/events/${encodeURIComponent(boot.event_ref)}/invitees.json`),
        fetch(`/events/${encodeURIComponent(boot.event_ref)}/capacity.json`),
      ]);
      if (invRes.ok) {
        const data = (await invRes.json()) as { invitees: InviteeRow[] };
        setInvitees(data.invitees);
      }
      if (capRes.ok) {
        const data = (await capRes.json()) as {
          capacity: { public_confirmed: number; cap: number | null; self_signup_open: boolean };
        };
        setPublicConfirmed(data.capacity.public_confirmed);
      }
    } catch (err) {
      setError(`Refresh failed: ${String(err)}`);
    }
  };

  onMount(() => {
    // Render slot contents
  });

  const publish = async () => {
    setBusy("publish");
    setError(null);
    try {
      const data = await postJson<{ event: { status: string } }>(
        `/events/${encodeURIComponent(boot.event_ref)}/publish`,
      );
      setStatus(data.event.status);
      const el = document.querySelector('[data-test="event-status"]');
      if (el) el.textContent = data.event.status;
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  };

  const archive = async () => {
    setBusy("archive");
    setError(null);
    try {
      const data = await postJson<{ event: { status: string } }>(
        `/events/${encodeURIComponent(boot.event_ref)}/archive`,
      );
      setStatus(data.event.status);
      const el = document.querySelector('[data-test="event-status"]');
      if (el) el.textContent = data.event.status;
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  };

  const saveContent = async (ev: SubmitEvent) => {
    ev.preventDefault();
    setBusy("content");
    setError(null);
    try {
      const data = await postJson<{
        event: {
          title: string;
          subtitle: string | null;
          summary: string | null;
          details_markdown: string;
          approximate_location_name: string | null;
          location_name: string | null;
          address: string | null;
          map_url: string | null;
          display_capacity: boolean;
          notes_label: string;
          notes_caption: string | null;
          dietary_label: string;
          arrival_note_label: string;
          arrival_note_caption: string | null;
          allow_rsvp_edits: boolean;
        };
      }>(`/events/${encodeURIComponent(boot.event_ref)}`, {
        title: title().trim(),
        subtitle: nullable(subtitle()),
        summary: nullable(summary()),
        details_markdown: details(),
        approximate_location_name: nullable(approximateLocation()),
        location_name: nullable(locationName()),
        address: nullable(address()),
        map_url: nullable(mapUrl()),
        display_capacity: displayCapacity(),
        notes_label: notesLabel().trim(),
        notes_caption: nullable(notesCaption()),
        dietary_label: dietaryLabel().trim(),
        arrival_note_label: arrivalNoteLabel().trim(),
        arrival_note_caption: nullable(arrivalNoteCaption()),
        allow_rsvp_edits: allowRsvpEdits(),
      });
      setTitle(data.event.title);
      setSubtitle(data.event.subtitle ?? "");
      setSummary(data.event.summary ?? "");
      setDetails(data.event.details_markdown);
      setApproximateLocation(data.event.approximate_location_name ?? "");
      setLocationName(data.event.location_name ?? "");
      setAddress(data.event.address ?? "");
      setMapUrl(data.event.map_url ?? "");
      setDisplayCapacity(data.event.display_capacity);
      setNotesLabel(data.event.notes_label);
      setNotesCaption(data.event.notes_caption ?? "");
      setDietaryLabel(data.event.dietary_label);
      setArrivalNoteLabel(data.event.arrival_note_label);
      setArrivalNoteCaption(data.event.arrival_note_caption ?? "");
      setAllowRsvpEdits(data.event.allow_rsvp_edits);
      window.location.reload();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  };

  const saveSettings = async (ev: SubmitEvent) => {
    ev.preventDefault();
    setBusy("settings");
    setError(null);
    try {
      const cap = attendeeCap().trim();
      const data = await postJson<{
        event: {
          starts_at: string;
          ends_at: string;
          timezone: string;
          visibility: string;
          signup_mode: string;
          attendee_cap: number | null;
          self_signup_requires_approval: boolean;
        };
      }>(`/events/${encodeURIComponent(boot.event_ref)}`, {
        starts_at: startsAt(),
        ends_at: endsAt(),
        timezone: timezone(),
        visibility: visibility(),
        signup_mode: signupMode(),
        attendee_cap: cap ? Number(cap) : null,
        self_signup_requires_approval: requiresApproval(),
      });
      setStartsAt(data.event.starts_at);
      setEndsAt(data.event.ends_at);
      setTimezone(data.event.timezone);
      setVisibility(data.event.visibility);
      setSignupMode(data.event.signup_mode);
      setAttendeeCap(data.event.attendee_cap === null ? "" : String(data.event.attendee_cap));
      setRequiresApproval(data.event.self_signup_requires_approval);
      const visEl = document.querySelector('[data-test="event-visibility"]');
      if (visEl) visEl.textContent = data.event.visibility;
      const signupEl = document.querySelector('[data-test="event-signup-mode"]');
      if (signupEl) signupEl.textContent = data.event.signup_mode;
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  };

  const addInvitee = async (ev: SubmitEvent) => {
    ev.preventDefault();
    if (!newName().trim()) return;
    setBusy("add-invitee");
    setError(null);
    try {
      const data = await postJson<{ invitee: InviteeRow; rsvp_url: string }>(
        `/events/${encodeURIComponent(boot.event_ref)}/invitees`,
        {
          event_id: boot.event_id,
          display_name: newName().trim(),
          email: newEmail().trim() || null,
          phone: null,
          party_size_limit: newParty(),
        },
      );
      setCreatedLink({ name: data.invitee.display_name, url: data.rsvp_url });
      setNewName("");
      setNewEmail("");
      setNewParty(1);
      await refresh();
      setConfirmed((c) => c);
      setOverCap((c) => c);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  };

  const regenerateToken = async (id: string) => {
    setBusy(`regen-${id}`);
    setError(null);
    try {
      const data = await postJson<{ rsvp_url: string }>(
        `/events/${encodeURIComponent(boot.event_ref)}/invitees/${encodeURIComponent(id)}/regenerate`,
      );
      const invitee = invitees().find((i) => i.id === id);
      if (invitee)
        setCreatedLink({ name: invitee.display_name, url: data.rsvp_url });
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  };

  const approveInvitee = async (id: string) => {
    setBusy(`approve-${id}`);
    setError(null);
    try {
      const data = await postJson<{ invitee: InviteeRow }>(
        `/events/${encodeURIComponent(boot.event_ref)}/invitees/${encodeURIComponent(id)}/approve`,
      );
      setInvitees((list) => list.map((i) => (i.id === id ? data.invitee : i)));
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  };

  const createSignupToken = async () => {
    setBusy("signup-token");
    setError(null);
    try {
      const data = await postJson<{ signup_url: string }>(
        `/events/${encodeURIComponent(boot.event_ref)}/signup-token`,
      );
      setSignupUrl(data.signup_url);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  };

  // Put each section into its slot via portal-less manual render by attaching below the mount.
  return (
    <div class={styles.adminPanel}>
      <Show when={error()}>
        <div class={styles.msgErr} role="alert">
          {error()}
        </div>
      </Show>

      <section class={styles.adminSection}>
        <h3 class={styles.adminHeading}>Public page</h3>
        <form class={styles.form} onSubmit={saveContent}>
          <label class={styles.field}>
            <span class={styles.label}>Title</span>
            <input
              class={styles.input}
              type="text"
              value={title()}
              onInput={(e) => setTitle(e.currentTarget.value)}
              required
              data-test="admin-title-input"
            />
          </label>
          <div class={styles.adminRow}>
            <label class={styles.field}>
              <span class={styles.label}>Subtitle</span>
              <input
                class={styles.input}
                type="text"
                value={subtitle()}
                onInput={(e) => setSubtitle(e.currentTarget.value)}
                data-test="admin-subtitle-input"
              />
            </label>
            <label class={styles.field}>
              <span class={styles.label}>Summary</span>
              <input
                class={styles.input}
                type="text"
                value={summary()}
                onInput={(e) => setSummary(e.currentTarget.value)}
                data-test="admin-summary-input"
              />
            </label>
          </div>
          <label class={styles.field}>
            <span class={styles.label}>Details</span>
            <textarea
              class={styles.textarea}
              rows="6"
              value={details()}
              onInput={(e) => setDetails(e.currentTarget.value)}
              data-test="admin-details-input"
            />
          </label>
          <div class={styles.adminRow}>
            <label class={styles.field}>
              <span class={styles.label}>Approximate location</span>
              <input
                class={styles.input}
                type="text"
                value={approximateLocation()}
                onInput={(e) => setApproximateLocation(e.currentTarget.value)}
                data-test="admin-approx-location-input"
              />
            </label>
            <label class={styles.field}>
              <span class={styles.label}>Exact location name</span>
              <input
                class={styles.input}
                type="text"
                value={locationName()}
                onInput={(e) => setLocationName(e.currentTarget.value)}
                data-test="admin-location-input"
              />
            </label>
            <label class={styles.field}>
              <span class={styles.label}>Address</span>
              <input
                class={styles.input}
                type="text"
                value={address()}
                onInput={(e) => setAddress(e.currentTarget.value)}
                data-test="admin-address-input"
              />
            </label>
            <label class={styles.field}>
              <span class={styles.label}>Map URL</span>
              <input
                class={styles.input}
                type="url"
                value={mapUrl()}
                onInput={(e) => setMapUrl(e.currentTarget.value)}
                data-test="admin-map-url-input"
              />
            </label>
          </div>
          <div class={styles.adminRow}>
            <label class={styles.field}>
              <span class={styles.label}>Notes label</span>
              <input
                class={styles.input}
                type="text"
                value={notesLabel()}
                onInput={(e) => setNotesLabel(e.currentTarget.value)}
                required
                data-test="admin-notes-label-input"
              />
            </label>
            <label class={styles.field}>
              <span class={styles.label}>Notes caption</span>
              <input
                class={styles.input}
                type="text"
                value={notesCaption()}
                onInput={(e) => setNotesCaption(e.currentTarget.value)}
                data-test="admin-notes-caption-input"
              />
            </label>
          </div>
          <div class={styles.adminRow}>
            <label class={styles.field}>
              <span class={styles.label}>Dietary label</span>
              <input
                class={styles.input}
                type="text"
                value={dietaryLabel()}
                onInput={(e) => setDietaryLabel(e.currentTarget.value)}
                required
                data-test="admin-dietary-label-input"
              />
            </label>
            <label class={styles.field}>
              <span class={styles.label}>Arrival note label</span>
              <input
                class={styles.input}
                type="text"
                value={arrivalNoteLabel()}
                onInput={(e) => setArrivalNoteLabel(e.currentTarget.value)}
                required
                data-test="admin-arrival-label-input"
              />
            </label>
            <label class={styles.field}>
              <span class={styles.label}>Arrival note caption</span>
              <input
                class={styles.input}
                type="text"
                value={arrivalNoteCaption()}
                onInput={(e) => setArrivalNoteCaption(e.currentTarget.value)}
                data-test="admin-arrival-caption-input"
              />
            </label>
          </div>
          <div class={styles.adminButtons}>
            <label class={styles.inlineToggle}>
              <input
                type="checkbox"
                checked={displayCapacity()}
                onChange={(e) => setDisplayCapacity(e.currentTarget.checked)}
                data-test="admin-display-capacity-input"
              />
              <span>Show public capacity</span>
            </label>
            <label class={styles.inlineToggle}>
              <input
                type="checkbox"
                checked={allowRsvpEdits()}
                onChange={(e) => setAllowRsvpEdits(e.currentTarget.checked)}
                data-test="admin-allow-rsvp-edits-input"
              />
              <span>Allow RSVP edits</span>
            </label>
          </div>
          <div class={styles.actions}>
            <button
              type="submit"
              class={styles.btnPrimary}
              disabled={busy() === "content"}
              data-test="admin-content-submit"
            >
              Save public page
            </button>
          </div>
        </form>
      </section>

      <section class={styles.adminSection}>
        <h3 class={styles.adminHeading}>Event settings</h3>
        <form class={styles.form} onSubmit={saveSettings}>
          <div class={styles.adminRow}>
            <label class={styles.field}>
              <span class={styles.label}>Starts at</span>
              <div class={styles.dateTimePair}>
                <input
                  class={styles.input}
                  type="date"
                  value={rfc3339ToDateValue(startsAt())}
                  onInput={(e) => setStartsAt(updateDatePart(startsAt(), e.currentTarget.value, "18:00"))}
                  data-test="admin-starts-date"
                />
                <input
                  class={styles.input}
                  type="time"
                  value={rfc3339ToTimeValue(startsAt())}
                  onInput={(e) => setStartsAt(updateTimePart(startsAt(), e.currentTarget.value))}
                  data-test="admin-starts-time"
                />
                <input
                  class={styles.input}
                  type="text"
                  value={startsAt()}
                  onInput={(e) => setStartsAt(e.currentTarget.value)}
                  required
                  data-test="admin-starts-input"
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
                  data-test="admin-ends-date"
                />
                <input
                  class={styles.input}
                  type="time"
                  value={rfc3339ToTimeValue(endsAt())}
                  onInput={(e) => setEndsAt(updateTimePart(endsAt(), e.currentTarget.value))}
                  data-test="admin-ends-time"
                />
                <input
                  class={styles.input}
                  type="text"
                  value={endsAt()}
                  onInput={(e) => setEndsAt(e.currentTarget.value)}
                  required
                  data-test="admin-ends-input"
                />
              </div>
            </label>
          </div>
          <div class={styles.adminRow}>
            <label class={styles.field}>
              <span class={styles.label}>Timezone</span>
              <input
                class={styles.input}
                type="text"
                value={timezone()}
                onInput={(e) => setTimezone(e.currentTarget.value)}
                required
                data-test="admin-timezone-input"
              />
            </label>
            <label class={styles.field}>
              <span class={styles.label}>Visibility</span>
              <select
                class={styles.input}
                value={visibility()}
                onChange={(e) => setVisibility(e.currentTarget.value)}
                data-test="admin-visibility-input"
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
                onChange={(e) => setSignupMode(e.currentTarget.value)}
                data-test="admin-signup-mode-input"
              >
                <option value="invite_only">Invite only</option>
                <option value="self_signup">Self signup</option>
              </select>
            </label>
            <label class={styles.field}>
              <span class={styles.label}>Attendee cap</span>
              <input
                class={styles.input}
                type="number"
                min="1"
                value={attendeeCap()}
                onInput={(e) => setAttendeeCap(e.currentTarget.value)}
                data-test="admin-attendee-cap-input"
              />
            </label>
          </div>
          <div class={styles.adminButtons}>
            <label class={styles.inlineToggle}>
              <input
                type="checkbox"
                checked={requiresApproval()}
                onChange={(e) => setRequiresApproval(e.currentTarget.checked)}
                data-test="admin-requires-approval-input"
              />
              <span>Require approval for self-signup</span>
            </label>
          </div>
          <div class={styles.actions}>
            <button
              type="submit"
              class={styles.btnPrimary}
              disabled={busy() === "settings"}
              data-test="admin-settings-submit"
            >
              Save settings
            </button>
          </div>
        </form>
      </section>

      <div class={styles.adminRow}>
        <section class={styles.adminSection}>
          <h3 class={styles.adminHeading}>Status</h3>
          <p class={styles.adminLine}>
            Current: <strong data-test="admin-status">{status()}</strong>
          </p>
          <div class={styles.adminButtons}>
            <button
              type="button"
              class={styles.btnPrimary}
              disabled={busy() !== null || status() === "published"}
              onClick={publish}
              data-test="publish-btn"
            >
              Publish
            </button>
            <button
              type="button"
              class={styles.btnGhost}
              disabled={busy() !== null || status() === "archived"}
              onClick={archive}
              data-test="archive-btn"
            >
              Archive
            </button>
          </div>
        </section>

        <section class={styles.adminSection}>
          <h3 class={styles.adminHeading}>Capacity</h3>
          <p class={styles.adminLine}>
            Public: <strong>{publicConfirmed()}</strong>
            <Show when={boot.cap !== null}> / {boot.cap}</Show>
          </p>
          <p class={styles.adminLine}>
            Actual confirmed: <strong>{confirmed()}</strong>
            <Show when={overCap() > 0}>
              <span class={styles.adminWarn}> ({overCap()} over cap)</span>
            </Show>
          </p>
        </section>
      </div>

      <section class={styles.adminSection}>
        <h3 class={styles.adminHeading}>Invitees ({invitees().length})</h3>
        <form class={styles.adminForm} onSubmit={addInvitee}>
          <input
            class={styles.input}
            type="text"
            placeholder="Name"
            value={newName()}
            onInput={(e) => setNewName(e.currentTarget.value)}
            required
            data-test="new-invitee-name"
          />
          <input
            class={styles.input}
            type="email"
            placeholder="Email (optional)"
            value={newEmail()}
            onInput={(e) => setNewEmail(e.currentTarget.value)}
          />
          <input
            class={styles.input}
            type="number"
            min="1"
            max="20"
            value={newParty()}
            onInput={(e) => setNewParty(Number(e.currentTarget.value) || 1)}
            aria-label="Party size limit"
          />
          <button
            type="submit"
            class={styles.btnPrimary}
            disabled={busy() === "add-invitee"}
            data-test="add-invitee-btn"
          >
            Add invitee
          </button>
        </form>

        <Show when={createdLink()}>
          {(link) => (
            <div class={styles.linkReveal} data-test="invite-link-reveal">
              <p>
                RSVP link for <strong>{link().name}</strong> (shown once):
              </p>
              <code class={styles.linkCode}>{link().url}</code>
              <button
                type="button"
                class={styles.btnGhost}
                onClick={() => {
                  void navigator.clipboard.writeText(link().url);
                }}
              >
                Copy
              </button>
            </div>
          )}
        </Show>

        <Show when={invitees().length > 0}>
          <table class={styles.adminTable}>
            <thead>
              <tr>
                <th>Name</th>
                <th>Email</th>
                <th>RSVP</th>
                <th>Location</th>
                <th>Party</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              <For each={invitees()}>
                {(inv) => (
                  <tr data-test={`invitee-row-${inv.id}`}>
                    <td>{inv.display_name}</td>
                    <td>{inv.email ?? ""}</td>
                    <td data-test={`invitee-status-${inv.id}`}>{inv.rsvp_status}</td>
                    <td>{inv.location_approved ? "approved" : "pending"}</td>
                    <td>{inv.party_size_limit}</td>
                    <td>
                      <div class={styles.adminButtons}>
                        <Show when={!inv.location_approved}>
                          <button
                            type="button"
                            class={styles.btnPrimary}
                            onClick={() => void approveInvitee(inv.id)}
                            disabled={busy() !== null}
                            data-test={`approve-invitee-${inv.id}`}
                          >
                            Approve
                          </button>
                        </Show>
                        <button
                          type="button"
                          class={styles.btnGhost}
                          onClick={() => void regenerateToken(inv.id)}
                          disabled={busy() !== null}
                        >
                          New link
                        </button>
                      </div>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </Show>
      </section>

      <section class={styles.adminSection}>
        <h3 class={styles.adminHeading}>Self-signup link</h3>
        <p class={styles.adminLine}>
          Mode: {signupMode()} · Approval: {requiresApproval() ? "required" : "not required"}
        </p>
        <div class={styles.adminButtons}>
          <button
            type="button"
            class={styles.btnPrimary}
            onClick={createSignupToken}
            disabled={busy() === "signup-token"}
            data-test="signup-token-btn"
          >
            Generate signup link
          </button>
        </div>
        <Show when={signupUrl()}>
          {(url) => (
            <div class={styles.linkReveal} data-test="signup-link-reveal">
              <p>Signup URL (shown once):</p>
              <code class={styles.linkCode}>{url()}</code>
            </div>
          )}
        </Show>
      </section>
    </div>
  );
}
