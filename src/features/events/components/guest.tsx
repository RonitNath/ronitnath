'use client';

/* The guest's side. Two client components: the answer form, which renders its
 * reply beside itself, and the clock, which is the only thing on the page
 * that the server cannot get right on its own — the server renders the time
 * in the event's zone and the browser re-renders it in the reader's, so a
 * guest in another city is never quietly told the wrong hour. */

import { useActionState, useEffect, useState } from 'react';

import type { FormState } from '@/features/auth/form-state';
import { respond } from '../respond';

const EMPTY: FormState = {};

export function LocalTime({
  startsAt,
  endsAt,
  timezone,
  server,
}: {
  startsAt: string;
  endsAt: string | null;
  timezone: string;
  server: string;
}) {
  const [text, setText] = useState(server);
  useEffect(() => {
    const here = Intl.DateTimeFormat().resolvedOptions().timeZone;
    if (!here || here === timezone) return;
    const start = new Date(startsAt);
    const day = new Intl.DateTimeFormat(undefined, {
      weekday: 'long',
      month: 'long',
      day: 'numeric',
      timeZone: here,
    }).format(start);
    const clock = new Intl.DateTimeFormat(undefined, {
      hour: 'numeric',
      minute: '2-digit',
      timeZone: here,
    });
    const zone =
      new Intl.DateTimeFormat(undefined, { timeZone: here, timeZoneName: 'short' })
        .formatToParts(start)
        .find((part) => part.type === 'timeZoneName')?.value ?? here;
    const tail = endsAt ? ` – ${clock.format(new Date(endsAt))}` : '';
    setText(`${day}, ${clock.format(start)}${tail} ${zone}`);
  }, [startsAt, endsAt, timezone]);
  return (
    <time dateTime={startsAt} suppressHydrationWarning>
      {text}
    </time>
  );
}

export function AnswerForm({
  slug,
  token,
  answer,
  plusOne,
  note,
  needsName,
  plusOneAllowed,
}: {
  slug: string;
  token: string;
  answer: 'yes' | 'maybe' | 'no' | null;
  plusOne: number;
  note: string;
  /* An open link knows the event and not the guest, so the guest says who
   * they are before they say whether they are coming. */
  needsName: boolean;
  plusOneAllowed: boolean;
}) {
  const [state, action, pending] = useActionState(respond, EMPTY);
  const [chosen, setChosen] = useState<'yes' | 'maybe' | 'no' | null>(answer);
  return (
    <form className="answer" action={action}>
      <input type="hidden" name="slug" value={slug} />
      <input type="hidden" name="token" value={token} />
      {needsName ? (
        <div className="field">
          <label htmlFor="guest-name">Your name</label>
          <input id="guest-name" name="name" type="text" maxLength={120} required />
        </div>
      ) : null}
      <fieldset className="answers">
        <legend>Coming?</legend>
        {(['yes', 'maybe', 'no'] as const).map((value) => (
          <label key={value} className="choice" data-chosen={chosen === value}>
            <input
              type="radio"
              name="response"
              value={value}
              defaultChecked={answer === value}
              onChange={() => setChosen(value)}
              required
            />
            {value === 'yes' ? 'Yes' : value === 'maybe' ? 'Maybe' : 'No'}
          </label>
        ))}
      </fieldset>
      {plusOneAllowed ? (
        <div className="field narrow">
          <label htmlFor="guest-plus">Bringing</label>
          <input
            id="guest-plus"
            name="plusOne"
            type="number"
            min={0}
            max={9}
            defaultValue={plusOne}
          />
        </div>
      ) : (
        <input type="hidden" name="plusOne" value="0" />
      )}
      <div className="field">
        <label htmlFor="guest-note">A note for the host</label>
        <input id="guest-note" name="note" type="text" maxLength={500} defaultValue={note} />
      </div>
      {state.error ? (
        <p className="note" data-state="invalid" role="alert">
          {state.error}
        </p>
      ) : null}
      <button type="submit" className="commit" disabled={pending}>
        {answer ? 'Change my answer' : 'Answer'}
      </button>
    </form>
  );
}
