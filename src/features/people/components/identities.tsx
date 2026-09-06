'use client';

/* The account's own controls: what it is called, which doors reach it, and
 * the one question the model can ask a member — "somebody holds a contact
 * that looks like you". */

import { useActionState } from 'react';

import type { FormState } from '@/features/auth/form-state';
import { addEmail, confirmMatch, removeIdentity, setDisplayName } from '../actions';

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

export function DisplayNameForm({ displayName }: { displayName: string }) {
  const [state, action, pending] = useActionState(setDisplayName, EMPTY);
  return (
    <form className="inline-form" action={action}>
      <div className="field">
        <label htmlFor="display-name">Name</label>
        <input
          id="display-name"
          name="displayName"
          type="text"
          defaultValue={displayName}
          autoComplete="name"
          required
        />
      </div>
      <button type="submit" className="commit" disabled={pending}>
        Save
      </button>
      <Note state={state} />
    </form>
  );
}

export function AddEmailForm() {
  const [state, action, pending] = useActionState(addEmail, EMPTY);
  return (
    <form className="inline-form" action={action}>
      <div className="field">
        <label htmlFor="add-email">Add an address</label>
        <input id="add-email" name="email" type="email" inputMode="email" required />
      </div>
      <button type="submit" className="commit" disabled={pending}>
        Send the link
      </button>
      <Note state={state} />
    </form>
  );
}

export function RemoveIdentityButton({ identity }: { identity: string }) {
  const [state, action, pending] = useActionState(removeIdentity, EMPTY);
  return (
    <form action={action}>
      <input type="hidden" name="identity" value={identity} />
      <button type="submit" className="linkish" disabled={pending}>
        Remove
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

export function ConfirmMatchButton({ match }: { match: string }) {
  const [state, action, pending] = useActionState(confirmMatch, EMPTY);
  if (state.notice) return <span className="note">{state.notice}</span>;
  return (
    <form action={action}>
      <input type="hidden" name="match" value={match} />
      <button type="submit" className="commit" disabled={pending}>
        That is me
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
