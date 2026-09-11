'use client';
import { useActionState } from 'react';
import { publishHome, saveAnnouncement, saveHome, type ConfigurationResult } from './actions';

const EMPTY: ConfigurationResult = {};
function Result({ state }: { state: ConfigurationResult }) {
  return (
    <span className="note" data-state={state.error ? 'invalid' : undefined}>
      {state.error ?? state.notice}
    </span>
  );
}

export function HomeEditor({
  version,
  body,
}: {
  version: number;
  body: { heading: string; tagline: string };
}) {
  const [saved, save, saving] = useActionState(saveHome, EMPTY);
  const [published, publish, publishing] = useActionState(publishHome, EMPTY);
  return (
    <div className="configuration-editor">
      <p className="note">Draft → publish · current draft revision {version}</p>
      <form action={save} className="filters">
        <input type="hidden" name="version" value={version} />
        <label>
          Heading <input name="heading" defaultValue={body.heading} />
        </label>
        <label>
          Tagline <input name="tagline" defaultValue={body.tagline} />
        </label>
        <button className="commit" disabled={saving}>
          Save draft
        </button>
        <Result state={saved} />
      </form>
      <form action={publish} className="command-row">
        <input type="hidden" name="version" value={version} />
        <button className="commit" disabled={publishing || version === 0}>
          Publish revision {version}
        </button>
        <Result state={published} />
      </form>
    </div>
  );
}

export function AnnouncementEditor({
  version,
  body,
}: {
  version: number;
  body: { enabled: boolean; text: string };
}) {
  const [state, action, pending] = useActionState(saveAnnouncement, EMPTY);
  return (
    <form action={action} className="filters">
      <input type="hidden" name="version" value={version} />
      <label>
        <input type="checkbox" name="enabled" defaultChecked={body.enabled} /> Enabled
      </label>
      <label>
        Text <input name="text" defaultValue={body.text} />
      </label>
      <button className="commit" disabled={pending}>
        Save live
      </button>
      <Result state={state} />
    </form>
  );
}
