'use client';

import { useActionState } from 'react';

import { revokeSession } from '@/features/auth/actions';
import type { FormState } from '@/features/auth/form-state';

const EMPTY: FormState = {};

/* One button per row. The public session id is what crosses the wire; the
 * integer join key stays on the server (docs/design.md). */
export function RevokeButton({ session }: { session: string }) {
  const [state, action, pending] = useActionState(revokeSession, EMPTY);
  return (
    <form action={action}>
      <input type="hidden" name="session" value={session} />
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
