'use client';
import { useActionState } from 'react';
import { Button, Checkbox, Field } from '@isoastra/ui';
import { RealtimeStatus } from '@isoastra/ui-realtime';
import { publishHome, saveAnnouncement, saveHome, type ConfigurationResult } from './actions';

const EMPTY: ConfigurationResult = {};
function Result({ state }: { state: ConfigurationResult }) {
  if (!state.error && !state.notice) return null;
  const conflict = state.error?.includes('another window') || state.error?.includes('moved');
  return <RealtimeStatus state={{ transport:'connected', freshness:'current', mutation:state.error?'rejected':'acknowledged', edit:conflict?'conflict':'pristine', access:'writable', message:state.error ?? state.notice }} />;
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
        <Field label="Heading" name="heading" defaultValue={body.heading} />
        <Field label="Tagline" name="tagline" defaultValue={body.tagline} />
        <Button type="submit" tone="primary" pending={saving}>Save draft</Button>
        <Result state={saved} />
      </form>
      <form action={publish} className="command-row">
        <input type="hidden" name="version" value={version} />
        <Button type="submit" tone="primary" pending={publishing} isDisabled={version === 0}>Publish revision {version}</Button>
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
      <Checkbox name="enabled" defaultSelected={body.enabled}>Enabled</Checkbox>
      <Field label="Text" name="text" defaultValue={body.text} />
      <Button type="submit" tone="primary" pending={pending}>Save live</Button>
      <Result state={state} />
    </form>
  );
}
