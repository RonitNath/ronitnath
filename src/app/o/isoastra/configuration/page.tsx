import { AnnouncementEditor, HomeEditor } from '@/features/configuration/editor';
import {
  affectedSubscribers,
  ANNOUNCEMENT_DEFAULT,
  configurationDefinitions,
  configurationState,
  HOME_DEFAULT,
} from '@/features/configuration/model';
import { requireOperator } from '@/lib/tiers';

export const dynamic = 'force-dynamic';
export default async function ConfigurationPage() {
  await requireOperator();
  const [state, homeAffected, announcementAffected] = await Promise.all([
    configurationState(),
    affectedSubscribers('homepage'),
    affectedSubscribers('announcement'),
  ]);
  const homeDraft =
    state.homeDraft.body && typeof state.homeDraft.body === 'object'
      ? (state.homeDraft.body as typeof HOME_DEFAULT)
      : state.home;
  const announcementDraft =
    state.announcementDraft.body && typeof state.announcementDraft.body === 'object'
      ? (state.announcementDraft.body as typeof ANNOUNCEMENT_DEFAULT)
      : state.announcement;
  return (
    <main className="indoors" data-realtime-config="homepage:published">
      <h1>Configuration</h1>
      <p className="note">
        Typed settings, their publication policy, and the views currently depending on them.
      </p>
      <section data-realtime-section="definitions">
        <h2>Definitions</h2>
        <div className="scroller">
          <table className="rows dense">
            <thead>
              <tr>
                <th>Key</th>
                <th>Scope</th>
                <th>Policy</th>
                <th>Dependencies</th>
              </tr>
            </thead>
            <tbody>
              {configurationDefinitions.map((definition) => (
                <tr key={definition.key}>
                  <td className="mono">{definition.key}</td>
                  <td>{definition.scope}</td>
                  <td>{definition.policy}</td>
                  <td>{definition.dependencies.join(', ')}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
      <section data-realtime-section="homepage">
        <h2>Homepage copy</h2>
        <p className="note">Published · {homeAffected.length} affected view(s)</p>
        <HomeEditor version={state.homeDraft.version} body={homeDraft} />
      </section>
      <section data-realtime-section="announcement">
        <h2>Announcement</h2>
        <p className="note">Live on save · {announcementAffected.length} affected view(s)</p>
        <AnnouncementEditor
          version={state.announcementDraft.version}
          body={announcementDraft}
        />
      </section>
      <section data-realtime-section="history">
        <h2>Published history</h2>
        <div className="scroller">
          <table className="rows dense">
            <thead>
              <tr>
                <th>Key</th>
                <th>Revision</th>
                <th>At</th>
                <th>Value</th>
              </tr>
            </thead>
            <tbody>
              {state.history.map((row) => (
                <tr key={`${row.key}:${row.version}`}>
                  <td>{row.key}</td>
                  <td className="mono">{row.version}</td>
                  <td className="mono">{row.at.toISOString()}</td>
                  <td>
                    <pre className="payload">{JSON.stringify(row.body, null, 2)}</pre>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </main>
  );
}
