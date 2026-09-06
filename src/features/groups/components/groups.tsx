'use client';

/* The controls on `/app/groups` and on an organization's page. */

import { useActionState } from 'react';

import type { FormState } from '@/features/auth/form-state';
import { Note } from '@/features/organizations/components/org';
import { addToGroup, createGroup, deleteGroup, removeFromGroup, renameGroup } from '../actions';

const EMPTY: FormState = {};

export interface Owner {
  value: string;
  label: string;
}

export function CreateGroupForm({ owners }: { owners: Owner[] }) {
  const [state, action, pending] = useActionState(createGroup, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>New group</h2>
      <div className="field">
        <label htmlFor="group-name">Name</label>
        <input id="group-name" name="name" type="text" autoComplete="off" required />
      </div>
      {owners.length > 1 ? (
        <div className="field">
          <label htmlFor="group-owner">Owner</label>
          <select id="group-owner" name="owner" defaultValue="me">
            {owners.map((owner) => (
              <option key={owner.value} value={owner.value}>
                {owner.label}
              </option>
            ))}
          </select>
        </div>
      ) : (
        <input type="hidden" name="owner" value="me" />
      )}
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        Create
      </button>
    </form>
  );
}

export function RenameGroupForm({ group, name }: { group: string; name: string }) {
  const [state, action, pending] = useActionState(renameGroup, EMPTY);
  return (
    <form className="inline-form" action={action}>
      <input type="hidden" name="group" value={group} />
      <div className="field">
        <label htmlFor={`rename-${group}`}>Name</label>
        <input id={`rename-${group}`} name="name" type="text" defaultValue={name} required />
      </div>
      <button type="submit" className="commit" disabled={pending}>
        Rename
      </button>
      <Note state={state} />
    </form>
  );
}

export interface Candidate {
  value: string;
  label: string;
}

export function AddToGroupForm({ group, candidates }: { group: string; candidates: Candidate[] }) {
  const [state, action, pending] = useActionState(addToGroup, EMPTY);
  return (
    <form className="inline-form" action={action}>
      <input type="hidden" name="group" value={group} />
      <div className="field">
        <label htmlFor={`add-${group}`}>Add</label>
        <select id={`add-${group}`} name="member" required>
          <option value="">Somebody</option>
          {candidates.map((candidate) => (
            <option key={candidate.value} value={candidate.value}>
              {candidate.label}
            </option>
          ))}
        </select>
      </div>
      <button type="submit" className="commit" disabled={pending}>
        Add
      </button>
      <Note state={state} />
    </form>
  );
}

export function RemoveFromGroupButton({ group, member }: { group: string; member: string }) {
  const [state, action, pending] = useActionState(removeFromGroup, EMPTY);
  if (state.notice) return <span className="note">{state.notice}</span>;
  return (
    <form action={action}>
      <input type="hidden" name="group" value={group} />
      <input type="hidden" name="member" value={member} />
      <button type="submit" className="linkish" disabled={pending}>
        Remove
      </button>
    </form>
  );
}

export function DeleteGroupButton({ group }: { group: string }) {
  const [state, action, pending] = useActionState(deleteGroup, EMPTY);
  if (state.notice) return <span className="note">{state.notice}</span>;
  return (
    <form action={action}>
      <input type="hidden" name="group" value={group} />
      <button type="submit" className="linkish" disabled={pending}>
        Delete
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
