import type { Metadata } from 'next';
import Link from 'next/link';

import { listEvents } from '@/features/events/queries';
import { readableWindow } from '@/features/events/time';
import { encodeId } from '@/lib/ids';
import { userPath } from '@/lib/paths';
import { requireSubjectPerson } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Events' };

export default async function EventsPage({ params }: { params: Promise<{ user: string }> }) {
  const { user } = await params;
  const { subjectPersonId, viewingOther } = await requireSubjectPerson(user, 'events');
  const events = await listEvents(subjectPersonId);

  return (
    <main className="indoors">
      <h1>Events</h1>
      <p className="note">
        Something you are hosting. Creating one is not publishing it; publishing mints the links you
        paste where you already talk to people.
      </p>

      <section>
        <h2>Yours</h2>
        <div className="scroller">
          <table className="rows wide">
            <thead>
              <tr>
                <th>Event</th>
                <th>When</th>
                <th>State</th>
                <th className="num">Yes</th>
                <th className="num">Maybe</th>
                <th className="num">Invited</th>
              </tr>
            </thead>
            <tbody>
              {events.map((row) => (
                <tr key={row.id}>
                  <td>
                    <Link href={userPath(user, `events/${encodeId('event', row.id)}`)}>{row.title}</Link>
                  </td>
                  <td>{readableWindow(row.startsAt, row.endsAt, row.timezone)}</td>
                  <td>
                    <span className="state" data-state={row.publishedAt ? 'claimed' : 'live'}>
                      {row.publishedAt ? 'Published' : 'Draft'}
                    </span>
                  </td>
                  <td className="num">{row.counts.yes}</td>
                  <td className="num">{row.counts.maybe}</td>
                  <td className="num">{row.invited}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {events.length === 0 ? (
          <p className="empty">
            Nothing yet. An event is a page you hand people a link to; it needs a title and a time
            to exist.
          </p>
        ) : null}
        {viewingOther ? null : (
          <p className="aside-line">
            <Link href={userPath(user, 'events/new')}>New event</Link>
          </p>
        )}
      </section>
    </main>
  );
}
