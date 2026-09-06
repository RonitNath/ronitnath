'use client';

/* The claim page's two doors. Which one a visitor sees is decided by the page;
 * both of them end in the same merge. */

import { useActionState } from 'react';

import type { FormState } from '@/features/auth/form-state';
import { claimByRegistering, claimLink } from '../actions';

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

/** For a visitor who is already signed in: one button. */
export function ClaimForm({ token, as }: { token: string; as: string }) {
  const [state, action, pending] = useActionState(claimLink, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>This is me</h2>
      <input type="hidden" name="token" value={token} />
      <p className="note">
        Signed in as <span className="mono">{as}</span>.
      </p>
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        This is me
      </button>
    </form>
  );
}

/** For a visitor with no account: the account and the claim in one step. */
export function ClaimRegisterForm({
  token,
  name,
  email,
}: {
  token: string;
  name: string;
  email: string;
}) {
  const [state, action, pending] = useActionState(claimByRegistering, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>This is me</h2>
      <input type="hidden" name="token" value={token} />
      <div className="field">
        <label htmlFor="claim-name">Name</label>
        <input
          id="claim-name"
          name="displayName"
          type="text"
          defaultValue={name}
          autoComplete="name"
          required
        />
      </div>
      <div className="field">
        <label htmlFor="claim-email">Email</label>
        <input
          id="claim-email"
          name="email"
          type="email"
          inputMode="email"
          defaultValue={email}
          autoComplete="username"
          required
        />
      </div>
      <div className="field">
        <label htmlFor="claim-password">Password</label>
        <input
          id="claim-password"
          name="password"
          type="password"
          autoComplete="new-password"
          minLength={10}
          required
        />
      </div>
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        Claim
      </button>
    </form>
  );
}
