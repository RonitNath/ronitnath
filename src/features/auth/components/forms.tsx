'use client';

/* The forms. Client components only because `useActionState` renders the
 * answer beside the field it came from and disables the button while the
 * command runs; the commands themselves are server actions, and every form
 * here submits without JavaScript as well.
 *
 * The ZITADEL button is a form now rather than a link. Better-auth builds the
 * authorization URL — state, PKCE and the redirect_uri that must match what
 * the app is registered with — so starting the round trip is a command, and a
 * command on this site is a server action. */

import { useActionState } from 'react';

import {
  requestPasswordReset,
  requestVerification,
  resetPassword,
  signInEmail,
  signInWithIsoastra,
  signUpEmail,
} from '@/features/auth/actions';
import { UNVERIFIED, type FormState } from '@/features/auth/form-state';

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

/* What the door says to a correct password against an unconfirmed address.
 * It takes the sign-in pane's place rather than sitting beside it: there is
 * one thing to do here, and a form cannot be nested inside another one. */
function ConfirmPane({ email }: { email: string }) {
  const [state, action, pending] = useActionState(requestVerification, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>Confirm your email</h2>
      <input type="hidden" name="email" value={email} />
      <p className="note">
        {UNVERIFIED} The link was sent to <span className="mono">{email}</span>.
      </p>
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        Send it again
      </button>
    </form>
  );
}

/* `email` is prefilled from a claim link, where the address is already known
 * and typing it again would be a test the visitor can fail. */
export function SignInForm({ next, email = '' }: { next: string; email?: string }) {
  const [state, action, pending] = useActionState(signInEmail, EMPTY);
  if (state.unverified && state.email) return <ConfirmPane email={state.email} />;
  return (
    <form className="pane" action={action}>
      <h2>Sign in</h2>
      <input type="hidden" name="next" value={next} />
      <div className="field">
        <label htmlFor="sign-in-email">Email</label>
        <input
          id="sign-in-email"
          name="email"
          type="email"
          inputMode="email"
          defaultValue={email}
          autoComplete="username"
          required
        />
      </div>
      <div className="field">
        <label htmlFor="sign-in-password">Password</label>
        <input
          id="sign-in-password"
          name="password"
          type="password"
          autoComplete="current-password"
          required
        />
      </div>
      <Note state={state.form === 'sign-in' ? state : EMPTY} />
      <button type="submit" className="commit" disabled={pending}>
        Sign in
      </button>
    </form>
  );
}

export function RegisterForm() {
  const [state, action, pending] = useActionState(signUpEmail, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>Register</h2>
      <div className="field">
        <label htmlFor="register-name">Name</label>
        <input id="register-name" name="displayName" type="text" autoComplete="name" required />
      </div>
      <div className="field">
        <label htmlFor="register-email">Email</label>
        <input
          id="register-email"
          name="email"
          type="email"
          inputMode="email"
          autoComplete="username"
          required
        />
      </div>
      <div className="field">
        <label htmlFor="register-password">Password</label>
        <input
          id="register-password"
          name="password"
          type="password"
          autoComplete="new-password"
          minLength={10}
          required
        />
      </div>
      <Note state={state.form === 'register' ? state : EMPTY} />
      <button type="submit" className="commit" disabled={pending}>
        Register
      </button>
    </form>
  );
}

/** The other door. Where it lands is where the reader was going. */
export function IsoastraButton({ next }: { next: string }) {
  const [state, action, pending] = useActionState(signInWithIsoastra, EMPTY);
  return (
    <form action={action}>
      <input type="hidden" name="next" value={next} />
      <button type="submit" className="federated" disabled={pending}>
        Sign in with Isoastra
      </button>
      <Note state={state} />
    </form>
  );
}

export function RequestResetForm() {
  const [state, action, pending] = useActionState(requestPasswordReset, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>Reset password</h2>
      <div className="field">
        <label htmlFor="reset-email">Email</label>
        <input
          id="reset-email"
          name="email"
          type="email"
          inputMode="email"
          autoComplete="username"
          required
        />
      </div>
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        Send the link
      </button>
    </form>
  );
}

export function ResetForm({ token }: { token: string }) {
  const [state, action, pending] = useActionState(resetPassword, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>Set a new password</h2>
      <input type="hidden" name="token" value={token} />
      <div className="field">
        <label htmlFor="new-password">New password</label>
        <input
          id="new-password"
          name="password"
          type="password"
          autoComplete="new-password"
          minLength={10}
          required
        />
      </div>
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        Set the password
      </button>
    </form>
  );
}

/** Confirming an address is a link click now, not a button press: better-auth
 *  verifies at its own endpoint and bounces the reader here. What is left for
 *  this form is the case where the link had already been used or had expired,
 *  which is another letter. */
export function ResendForm() {
  const [state, action, pending] = useActionState(requestVerification, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>Send another link</h2>
      <div className="field">
        <label htmlFor="verify-email">Email</label>
        <input
          id="verify-email"
          name="email"
          type="email"
          inputMode="email"
          autoComplete="username"
          required
        />
      </div>
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        Send it again
      </button>
    </form>
  );
}
