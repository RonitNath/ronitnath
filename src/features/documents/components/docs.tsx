'use client';

/* The controls on `/app/documents`. */

import { useActionState, useEffect, useState } from 'react';

import type { FormState } from '@/features/auth/form-state';
import { Note } from '@/features/organizations/components/org';
import {
  createDocument,
  editDocument,
  revokeShare,
  setPublication,
  shareDocument,
  transferDocument,
} from '../actions';

const EMPTY: FormState = {};

export interface Choice {
  value: string;
  label: string;
}

export function CreateDocumentForm({ owners }: { owners: Choice[] }) {
  const [state, action, pending] = useActionState(createDocument, EMPTY);
  return (
    <form className="pane" action={action}>
      <h2>New document</h2>
      <div className="field">
        <label htmlFor="doc-title">Title</label>
        <input id="doc-title" name="title" type="text" autoComplete="off" required />
      </div>
      {owners.length > 1 ? (
        <div className="field">
          <label htmlFor="doc-owner">Owner</label>
          <select id="doc-owner" name="owner" defaultValue="me">
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

export function EditDocumentForm({
  document,
  title,
  body,
}: {
  document: string;
  title: string;
  body: string;
}) {
  const [state, action, pending] = useActionState(editDocument, EMPTY);
  const [mutationId, setMutationId] = useState(() => crypto.randomUUID());
  useEffect(() => {
    if (state.notice) setMutationId(crypto.randomUUID());
  }, [state.notice]);
  return (
    <form className="pane event-form" action={action}>
      <input type="hidden" name="document" value={document} />
      <input type="hidden" name="mutationId" value={mutationId} />
      <input type="hidden" name="originalTitle" value={title} />
      <input type="hidden" name="originalBody" value={body} />
      <div className="field">
        <label htmlFor="edit-title">Title</label>
        <input id="edit-title" name="title" type="text" defaultValue={title} required />
      </div>
      <div className="field">
        <label htmlFor="edit-body">Body</label>
        <textarea id="edit-body" name="body" rows={16} defaultValue={body} />
      </div>
      <Note state={state} />
      <button type="submit" className="commit" disabled={pending}>
        Save
      </button>
    </form>
  );
}

export function PublishButton({
  document,
  published,
  slug,
}: {
  document: string;
  published: boolean;
  slug: string | null;
}) {
  const [state, action, pending] = useActionState(setPublication, EMPTY);
  const [mutationId, setMutationId] = useState(() => crypto.randomUUID());
  useEffect(() => {
    if (state.notice) setMutationId(crypto.randomUUID());
  }, [state.notice]);
  return (
    <div className="publish">
      <form action={action}>
        <input type="hidden" name="document" value={document} />
        <input type="hidden" name="publish" value={published ? 'no' : 'yes'} />
        <input type="hidden" name="mutationId" value={mutationId} />
        <button type="submit" className="commit" disabled={pending}>
          {published ? 'Unpublish' : 'Publish'}
        </button>
      </form>
      {published && slug ? (
        <p className="note">
          Anybody can read it at <a href={`/d/${slug}`}>/d/{slug}</a>.
        </p>
      ) : null}
      <Note state={state} />
    </div>
  );
}

const LEVEL_OPTIONS = ['viewer', 'commenter', 'editor'] as const;

export function ShareForm({
  document,
  candidates,
}: {
  document: string;
  candidates: Choice[];
}) {
  const [state, action, pending] = useActionState(shareDocument, EMPTY);
  return (
    <form className="inline-form" action={action}>
      <input type="hidden" name="document" value={document} />
      <div className="field">
        <label htmlFor="share-subject">Share with</label>
        <select id="share-subject" name="subject" required>
          <option value="">Somebody</option>
          {candidates.map((candidate) => (
            <option key={candidate.value} value={candidate.value}>
              {candidate.label}
            </option>
          ))}
        </select>
      </div>
      <div className="field">
        <label htmlFor="share-level">As</label>
        <select id="share-level" name="level" defaultValue="viewer">
          {LEVEL_OPTIONS.map((level) => (
            <option key={level} value={level}>
              {level}
            </option>
          ))}
        </select>
      </div>
      <button type="submit" className="commit" disabled={pending}>
        Share
      </button>
      <Note state={state} />
    </form>
  );
}

export function RevokeShareButton({
  document,
  subject,
}: {
  document: string;
  subject: string;
}) {
  const [state, action, pending] = useActionState(revokeShare, EMPTY);
  if (state.notice) return <span className="note">{state.notice}</span>;
  return (
    <form action={action}>
      <input type="hidden" name="document" value={document} />
      <input type="hidden" name="subject" value={subject} />
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

export function TransferDocumentForm({
  document,
  candidates,
}: {
  document: string;
  candidates: Choice[];
}) {
  const [state, action, pending] = useActionState(transferDocument, EMPTY);
  if (candidates.length === 0) return null;
  return (
    <form className="inline-form" action={action}>
      <input type="hidden" name="document" value={document} />
      <div className="field">
        <label htmlFor="transfer-subject">Hand over to</label>
        <select id="transfer-subject" name="subject" required>
          <option value="">Somebody</option>
          {candidates.map((candidate) => (
            <option key={candidate.value} value={candidate.value}>
              {candidate.label}
            </option>
          ))}
        </select>
      </div>
      <button type="submit" className="commit" disabled={pending}>
        Transfer
      </button>
      <Note state={state} />
    </form>
  );
}
