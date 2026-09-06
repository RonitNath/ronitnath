'use client';

/* The controls on /app/people. Client components only because each one
 * renders its own answer beside itself; every form here submits without
 * JavaScript as well, and the one thing that needs the browser — copying the
 * minted URL — degrades to selecting the text. */

import { useActionState, useState } from 'react';

import type { FormState } from '@/features/auth/form-state';
import { holdPerson, invite, revokeLink } from '../actions';

const EMPTY: FormState = {};

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

export function HoldForm() {
  const [state, action, pending] = useActionState(holdPerson, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>Hold someone</h2>
      <div className="field">
        <label htmlFor="hold-handle">Email, phone, or name</label>
        <input id="hold-handle" name="handle" type="text" autoComplete="off" required />
      </div>
      <div className="field">
        <label htmlFor="hold-name">What you call them</label>
        <input id="hold-name" name="displayName" type="text" autoComplete="off" />
      </div>
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        Hold
      </button>
    </form>
  );
}

/* The URL exists on this page and nowhere else. Saying so is not a caveat:
 * a member who assumes it can be fetched again will close the tab. */
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

export function InviteButton({ person, label }: { person: string; label: string }) {
  const [state, action, pending] = useActionState(invite, EMPTY);
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
      <input type="hidden" name="person" value={person} />
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

export function RevokeLinkButton({ link }: { link: string }) {
  const [state, action, pending] = useActionState(revokeLink, EMPTY);
  if (state.notice) return <span className="note">{state.notice}</span>;
  return (
    <form action={action}>
      <input type="hidden" name="link" value={link} />
      <button type="submit" className="linkish" disabled={pending}>
        Revoke
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
