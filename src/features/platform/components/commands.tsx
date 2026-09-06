'use client';

/* One control shape for every operator command: hidden fields the page knows,
 * a written reason where the model asks for one, a verb, and the answer in
 * place. The re-authentication refusal is the only one that offers a way out
 * of itself — it is also the only one that says what it is. */

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useActionState } from 'react';

import type { FormState } from '@/features/auth/form-state';
import { reauthPath } from '../reauth';

const EMPTY: FormState = {};

type Action = (state: FormState, form: FormData) => Promise<FormState>;

export function Answer({ state }: { state: FormState }) {
  const here = usePathname();
  if (state.reauth) {
    return (
      <span className="note" data-state="invalid" role="alert">
        {' '}
        {state.error} <Link href={reauthPath(here)}>Confirm</Link>
      </span>
    );
  }
  if (state.error) {
    return (
      <span className="note" data-state="invalid" role="alert">
        {' '}
        {state.error}
      </span>
    );
  }
  if (state.notice) {
    return (
      <span className="note" data-state="done" role="status">
        {' '}
        {state.notice}
      </span>
    );
  }
  return null;
}

export interface CommandProps {
  action: Action;
  fields?: Record<string, string>;
  label: string;
  /* A written reason, stored in the audit payload. */
  reason?: string;
  /* Link weight in a table cell, button weight in a pane. */
  weight?: 'link' | 'commit';
  className?: string;
  children?: React.ReactNode;
}

export function Command({
  action,
  fields = {},
  label,
  reason,
  weight = 'link',
  className,
  children,
}: CommandProps) {
  const [state, run, pending] = useActionState(action, EMPTY);
  return (
    <form action={run} className={className}>
      {Object.entries(fields).map(([name, value]) => (
        <input key={name} type="hidden" name={name} value={value} />
      ))}
      {state.confirm ? <input type="hidden" name="confirmed" value={state.confirm} /> : null}
      {children}
      {reason === undefined ? null : (
        <input
          className="reason"
          name="reason"
          type="text"
          placeholder={reason}
          aria-label={reason}
          required
          maxLength={2000}
        />
      )}
      <button type="submit" className={weight === 'link' ? 'linkish' : 'commit'} disabled={pending}>
        {state.confirm ? `${label} anyway` : label}
      </button>
      <Answer state={state} />
    </form>
  );
}

/** A command over one of two named parties: the operator picks which survives
 *  before they can rule. */
export function ChoiceCommand({
  action,
  fields = {},
  label,
  name,
  choices,
  reason,
}: CommandProps & { name: string; choices: { value: string; label: string }[] }) {
  const [state, run, pending] = useActionState(action, EMPTY);
  return (
    <form action={run} className="command-row">
      {Object.entries(fields).map(([key, value]) => (
        <input key={key} type="hidden" name={key} value={value} />
      ))}
      {state.confirm ? <input type="hidden" name="confirmed" value={state.confirm} /> : null}
      <select name={name} aria-label={label} defaultValue={choices[0]?.value}>
        {choices.map((choice) => (
          <option key={choice.value} value={choice.value}>
            {choice.label}
          </option>
        ))}
      </select>
      {reason === undefined ? null : (
        <input
          className="reason"
          name="reason"
          type="text"
          placeholder={reason}
          aria-label={reason}
          required
          maxLength={2000}
        />
      )}
      <button type="submit" className="commit" disabled={pending}>
        {label}
      </button>
      <Answer state={state} />
    </form>
  );
}

/** Two parties and a reason: the operator's own proposal. */
export function PairCommand({
  action,
  label,
  choices,
}: {
  action: Action;
  label: string;
  choices: { value: string; label: string }[];
}) {
  const [state, run, pending] = useActionState(action, EMPTY);
  return (
    <form action={run} className="command-row">
      <select name="left" aria-label="One party" defaultValue={choices[0]?.value}>
        {choices.map((choice) => (
          <option key={choice.value} value={choice.value}>
            {choice.label}
          </option>
        ))}
      </select>
      <select name="right" aria-label="The other party" defaultValue={choices[1]?.value}>
        {choices.map((choice) => (
          <option key={choice.value} value={choice.value}>
            {choice.label}
          </option>
        ))}
      </select>
      <button type="submit" className="commit" disabled={pending}>
        {label}
      </button>
      <Answer state={state} />
    </form>
  );
}
