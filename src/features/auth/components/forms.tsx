'use client';

/* The forms. Client components only because `useActionState` renders the
 * answer beside the field it came from and disables the button while the
 * command runs; the commands themselves are server actions, and every form
 * here submits without JavaScript as well. */

import { useActionState } from 'react';

import {
  register,
  requestPasswordReset,
  resetPassword,
  signIn,
  verifyEmail,
} from '@/features/auth/actions';
import type { FormState } from '@/features/auth/form-state';

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

export function SignInForm({ next }: { next: string }) {
  const [state, action, pending] = useActionState(signIn, EMPTY);
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
  const [state, action, pending] = useActionState(register, EMPTY);
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

export function VerifyForm({ token }: { token: string }) {
  const [state, action, pending] = useActionState(verifyEmail, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>Confirm your email</h2>
      <input type="hidden" name="token" value={token} />
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        Confirm
      </button>
    </form>
  );
}
