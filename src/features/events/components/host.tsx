'use client';

/* The host's controls. Client components because each renders its answer
 * beside itself and the minted URLs exist only in the reply to the command
 * that made them; every form here submits without JavaScript as well. */

import { useActionState, useState } from 'react';

import type { FormState } from '@/features/auth/form-state';
import { useRefreshOn } from '@/features/realtime/use-events';
import {
  createEvent,
  invitePeople,
  mintGuestLink,
  removeInvite,
  setPublication,
  updateEvent,
} from '../actions';

const EMPTY: FormState = {};

/** The guest list, live.
 *
 *  A host watching their own page while people answer on their own phones is
 *  the site's first realtime surface, and it is the one that makes the case
 *  for the spine: the numbers in the heading, the word beside each guest and
 *  the line about capacity are all derived from rsvp rows, and a page that
 *  only told the truth when somebody pressed reload was quietly wrong the
 *  whole time it was open.
 *
 *  It listens on the host's own stream — every rsvp for an event they host is
 *  emitted there — and filters to the one resource kind, so an invitation
 *  being minted or a contact being renamed does not refetch this page. The
 *  answer to an event is `router.refresh()`: the frame says only that
 *  something moved, and the numbers are recomputed by the server component
 *  through the same authorized read path that drew them, so nothing here ever
 *  holds a fact that did not pass the gate.
 *
 *  It renders nothing. The list it refreshes is the page's own table — this
 *  is a subscription, not a widget, and a "live" badge beside a guest list is
 *  chrome explaining a mechanism rather than showing a fact. */
export function GuestListStream({ stream }: { stream: string }) {
  useRefreshOn(stream, ['rsvp']);
  return null;
}

function Note({ state }: { state: FormState }) {
  if (state.error) {
    return (
      <p className="note" data-state="invalid" role="alert">
        {state.error}
      </p>
    );
  }
  if (state.notice) {
    return (
      <p className="note" data-state="done" role="status">
        {state.notice}
      </p>
    );
  }
  return null;
}

/* A minted URL exists on this page and nowhere else. Saying so is not a
 * caveat: a host who assumes it can be fetched again will close the tab. */
function Minted({ url }: { url: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="minted">
      <code className="url">{url}</code>
      <button
        type="button"
        className="linkish"
        onClick={() => {
          void navigator.clipboard?.writeText(url).then(() => setCopied(true));
        }}
      >
        {copied ? 'Copied' : 'Copy'}
      </button>
    </div>
  );
}

export interface EventDraft {
  id?: string;
  title: string;
  startsAt: string;
  endsAt: string;
  timezone: string;
  location: string;
  address: string;
  body: string;
  capacity: string;
  colour: string;
  posterUrl: string;
  revealGuests: boolean;
}

const COLOURS = ['none', 'night', 'ember', 'gold', 'moss'] as const;

/** One form for both commands: creating and editing an event ask for exactly
 *  the same things, and a second form would be a second place for them to
 *  drift apart. */
export function EventForm({ draft }: { draft: EventDraft }) {
  const [state, action, pending] = useActionState(draft.id ? updateEvent : createEvent, EMPTY);
  return (
    <form className="pane event-form" action={action}>
      {draft.id ? <input type="hidden" name="event" value={draft.id} /> : null}
      <div className="field">
        <label htmlFor="event-title">Title</label>
        <input id="event-title" name="title" type="text" defaultValue={draft.title} maxLength={140} required />
      </div>
      <div className="pair">
        <div className="field">
          <label htmlFor="event-starts">Starts</label>
          <input id="event-starts" name="startsAt" type="datetime-local" defaultValue={draft.startsAt} required />
        </div>
        <div className="field">
          <label htmlFor="event-ends">Ends</label>
          <input id="event-ends" name="endsAt" type="datetime-local" defaultValue={draft.endsAt} />
        </div>
        <div className="field">
          <label htmlFor="event-zone">Time zone</label>
          <input id="event-zone" name="timezone" type="text" defaultValue={draft.timezone} required />
        </div>
      </div>
      <div className="pair">
        <div className="field">
          <label htmlFor="event-place">Place</label>
          <input id="event-place" name="location" type="text" defaultValue={draft.location} maxLength={200} />
        </div>
        <div className="field">
          <label htmlFor="event-address">Address, after yes</label>
          <input id="event-address" name="address" type="text" defaultValue={draft.address} maxLength={400} />
        </div>
        <div className="field">
          <label htmlFor="event-capacity">Capacity</label>
          <input id="event-capacity" name="capacity" type="text" inputMode="numeric" defaultValue={draft.capacity} />
        </div>
      </div>
      <div className="field">
        <label htmlFor="event-body">Body</label>
        <textarea id="event-body" name="body" rows={7} defaultValue={draft.body} maxLength={8000} />
      </div>
      {draft.id ? (
        <div className="pair">
          <div className="field">
            <label htmlFor="event-colour">Colour</label>
            <select id="event-colour" name="colour" defaultValue={draft.colour || 'none'}>
              {COLOURS.map((colour) => (
                <option key={colour} value={colour === 'none' ? '' : colour}>
                  {colour}
                </option>
              ))}
            </select>
          </div>
          <div className="field">
            <label htmlFor="event-poster">Poster URL</label>
            <input id="event-poster" name="posterUrl" type="url" defaultValue={draft.posterUrl} maxLength={500} />
          </div>
          <div className="field check">
            <label htmlFor="event-reveal">
              <input
                id="event-reveal"
                name="revealGuests"
                type="checkbox"
                defaultChecked={draft.revealGuests}
              />
              Full names after yes
            </label>
          </div>
        </div>
      ) : null}
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        {draft.id ? 'Save' : 'Create'}
      </button>
    </form>
  );
}

export function PublishButton({ event, published }: { event: string; published: boolean }) {
  const [state, action, pending] = useActionState(setPublication, EMPTY);
  return (
    <div className="publish">
      <form action={action}>
        <input type="hidden" name="event" value={event} />
        <input type="hidden" name="intent" value={published ? 'unpublish' : 'publish'} />
        <button type="submit" className="commit" disabled={pending}>
          {published ? 'Unpublish' : 'Publish'}
        </button>
      </form>
      <Note state={state} />
      {state.mintedLinks?.length ? (
        <ul className="minted-list">
          {state.mintedLinks.map((row) => (
            <li key={row.url}>
              <span className="who">{row.name}</span>
              <Minted url={row.url} />
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

export function InviteForm({ event }: { event: string }) {
  const [state, action, pending] = useActionState(invitePeople, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>Invite</h2>
      <div className="field">
        <label htmlFor="invite-people">One name, address or number per line</label>
        <textarea id="invite-people" name="people" rows={4} maxLength={4000} />
      </div>
      <input type="hidden" name="event" value={event} />
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        Invite
      </button>
    </form>
  );
}

export function LinkButton({
  event,
  person,
  label,
}: {
  event: string;
  person?: string;
  label: string;
}) {
  const [state, action, pending] = useActionState(mintGuestLink, EMPTY);
  if (state.minted) {
    return (
      <div className="minted-row">
        <Minted url={state.minted} />
        <p className="note">{state.notice}</p>
      </div>
    );
  }
  return (
    <form action={action}>
      <input type="hidden" name="event" value={event} />
      {person ? <input type="hidden" name="person" value={person} /> : null}
      <button type="submit" className="linkish" disabled={pending}>
        {label}
      </button>
      {state.error ? (
        <span className="note" data-state="invalid">
          {' '}
          {state.error}
        </span>
      ) : null}
    </form>
  );
}

export function RemoveInviteButton({ event, person }: { event: string; person: string }) {
  const [state, action, pending] = useActionState(removeInvite, EMPTY);
  if (state.notice) return <span className="note">{state.notice}</span>;
  return (
    <form action={action}>
      <input type="hidden" name="event" value={event} />
      <input type="hidden" name="person" value={person} />
      <button type="submit" className="linkish" disabled={pending}>
        Remove
      </button>
    </form>
  );
}
