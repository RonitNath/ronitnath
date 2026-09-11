'use client';

/* The two controls that touch pictures: the guest's drop zone and the host's
 * hide.
 *
 * The drop zone is a `<label>` wrapped around an `<input type="file">`, which
 * is already a drop target, a click target and a keyboard target without any
 * of the three being wired by hand — a file dropped on it is a file in the
 * field. The only script is the one that writes the chosen file's name into
 * the label, and the page works without it: the field is still a field and
 * "Add" still sends it.
 *
 * The verb is a separate button rather than a submit fired the moment a file
 * is chosen, so choosing and sending stay two acts: a guest who picks the
 * wrong picture out of the roll has somewhere to notice before it is on the
 * page.
 *
 * There is no progress bar. The form is disabled while it is in flight and the
 * verb says so; a bar drawn over an upload it cannot measure is a number
 * nobody witnessed.
 */

import { useActionState, useState } from 'react';

import type { FormState } from '@/features/auth/form-state';
import { addPhoto, setPhotoHidden } from '../photo-actions';

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

export function PhotoForm({ slug, token }: { slug: string; token: string }) {
  const [state, action, pending] = useActionState(addPhoto, EMPTY);
  const [chosen, setChosen] = useState('');
  return (
    <form className="add-photo" action={action}>
      <input type="hidden" name="slug" value={slug} />
      <input type="hidden" name="token" value={token} />
      <label className="drop">
        <input
          type="file"
          name="photo"
          accept="image/jpeg,image/png,image/webp"
          disabled={pending}
          onChange={(event) => setChosen(event.currentTarget.files?.[0]?.name ?? '')}
        />
        <span>{chosen || 'Add a picture'}</span>
      </label>
      <button type="submit" disabled={pending}>
        {pending ? 'Adding' : 'Add'}
      </button>
      <Note state={state} />
    </form>
  );
}

/** The host's control, on the picture it is about. One verb: the state is
 *  what the button offers to change, so there is nothing else to read. */
export function PhotoVisibility({
  event,
  photo,
  hidden,
}: {
  event: string;
  photo: string;
  hidden: boolean;
}) {
  const [, action, pending] = useActionState(setPhotoHidden, EMPTY);
  return (
    <form action={action}>
      <input type="hidden" name="event" value={event} />
      <input type="hidden" name="photo" value={photo} />
      <input type="hidden" name="hidden" value={hidden ? '0' : '1'} />
      <button type="submit" disabled={pending}>
        {hidden ? 'Show' : 'Hide'}
      </button>
    </form>
  );
}
