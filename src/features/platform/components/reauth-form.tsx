'use client';

import { useActionState } from 'react';

import type { FormState } from '@/features/auth/form-state';
import { reauthenticate } from '../actions';

const EMPTY: FormState = {};

/* The local operator's half of ReAuthenticate. The OIDC half is a link, not a
 * form: a password this deployment never stored cannot be asked for here. */
export function ReauthForm({ next }: { next: string }) {
  const [state, action, pending] = useActionState(reauthenticate, EMPTY);
  return (
    <form className="pane" action={action}>
      <input type="hidden" name="next" value={next} />
      <div className="field">
        <label htmlFor="reauth-password">Password</label>
        <input
          id="reauth-password"
          name="password"
          type="password"
          autoComplete="current-password"
          required
        />
      </div>
      <button type="submit" className="commit" disabled={pending}>
        Confirm
      </button>
      {state.error ? (
        <p className="note" data-state="invalid" role="alert">
          {state.error}
        </p>
      ) : null}
      {state.notice ? (
        <p className="note" data-state="done" role="status">
          {state.notice} <a href={next}>Back</a>
        </p>
      ) : null}
    </form>
  );
}
