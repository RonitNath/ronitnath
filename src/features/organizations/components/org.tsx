'use client';

/* The controls on `/app` and `/org/<handle>`. Client components only because
 * each renders its answer beside itself; every form here submits without
 * JavaScript as well, and the one thing that needs the browser — copying the
 * minted URL — degrades to selecting the text. */

import { useActionState, useState } from 'react';

import type { FormState } from '@/features/auth/form-state';
import {
  createOrganization,
  inviteToOrganization,
  leaveOrganization,
  removeMember,
  revokeInvitation,
  setOrganizationProfile,
  setRole,
  transferOrganization,
} from '../actions';

const EMPTY: FormState = {};

export function Note({ state }: { state: FormState }) {
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

export function Minted({ url }: { url: string }) {
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

export function CreateOrganizationForm() {
  const [state, action, pending] = useActionState(createOrganization, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>Start an organization</h2>
      <div className="field">
        <label htmlFor="org-name">Name</label>
        <input id="org-name" name="name" type="text" autoComplete="off" required />
      </div>
      <div className="field">
        <label htmlFor="org-handle">Handle</label>
        <input
          id="org-handle"
          name="handle"
          type="text"
          autoComplete="off"
          pattern="[A-Za-z0-9-]+"
          required
        />
      </div>
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        Create
      </button>
    </form>
  );
}

export function ProfileForm({ organization, name }: { organization: string; name: string }) {
  const [state, action, pending] = useActionState(setOrganizationProfile, EMPTY);
  return (
    <form className="inline-form" action={action}>
      <input type="hidden" name="organization" value={organization} />
      <div className="field">
        <label htmlFor="profile-name">Name</label>
        <input id="profile-name" name="name" type="text" defaultValue={name} required />
      </div>
      <button type="submit" className="commit" disabled={pending}>
        Save
      </button>
      <Note state={state} />
    </form>
  );
}

const ROLE_OPTIONS = ['member', 'admin', 'owner'] as const;

export function InviteForm({ organization }: { organization: string }) {
  const [state, action, pending] = useActionState(inviteToOrganization, EMPTY);
  return (
    <form className="inline-form" action={action}>
      <input type="hidden" name="organization" value={organization} />
      <div className="field">
        <label htmlFor="invite-handle">Email, phone, or name</label>
        <input id="invite-handle" name="handle" type="text" autoComplete="off" required />
      </div>
      <div className="field">
        <label htmlFor="invite-name">What you call them</label>
        <input id="invite-name" name="displayName" type="text" autoComplete="off" />
      </div>
      <div className="field">
        <label htmlFor="invite-role">Role</label>
        <select id="invite-role" name="role" defaultValue="member">
          {ROLE_OPTIONS.map((role) => (
            <option key={role} value={role}>
              {role}
            </option>
          ))}
        </select>
      </div>
      <button type="submit" className="commit" disabled={pending}>
        Invite
      </button>
      {state.minted ? (
        <div className="minted-row">
          <Minted url={state.minted} />
          <p className="note">{state.notice}</p>
        </div>
      ) : (
        <Note state={state} />
      )}
    </form>
  );
}

export function RoleForm({
  organization,
  person,
  role,
}: {
  organization: string;
  person: string;
  role: string;
}) {
  const [state, action, pending] = useActionState(setRole, EMPTY);
  return (
    <form action={action}>
      <input type="hidden" name="organization" value={organization} />
      <input type="hidden" name="person" value={person} />
      <select name="role" defaultValue={role} aria-label="Role">
        {ROLE_OPTIONS.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
      <button type="submit" className="linkish" disabled={pending}>
        Set
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

function Verb({
  action,
  label,
  fields,
}: {
  action: (prev: FormState, form: FormData) => Promise<FormState>;
  label: string;
  fields: Record<string, string>;
}) {
  const [state, run, pending] = useActionState(action, EMPTY);
  if (state.notice) return <span className="note">{state.notice}</span>;
  return (
    <form action={run}>
      {Object.entries(fields).map(([name, value]) => (
        <input key={name} type="hidden" name={name} value={value} />
      ))}
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

export function RemoveMemberButton({
  organization,
  person,
}: {
  organization: string;
  person: string;
}) {
  return <Verb action={removeMember} label="Remove" fields={{ organization, person }} />;
}

export function TransferButton({
  organization,
  person,
}: {
  organization: string;
  person: string;
}) {
  return <Verb action={transferOrganization} label="Hand over" fields={{ organization, person }} />;
}

export function RevokeInvitationButton({
  organization,
  link,
}: {
  organization: string;
  link: string;
}) {
  return <Verb action={revokeInvitation} label="Revoke" fields={{ organization, link }} />;
}

export function LeaveButton({ organization }: { organization: string }) {
  return <Verb action={leaveOrganization} label="Leave" fields={{ organization }} />;
}
