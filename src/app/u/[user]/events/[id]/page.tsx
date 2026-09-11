import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';

import { database } from '@/db/client';
import { hostedEvent } from '@/features/events/authority';
import { headcount, overflow } from '@/features/events/capacity';
import {
  EventForm,
  GuestListStream,
  InviteForm,
  LinkButton,
  PublishButton,
  RemoveInviteButton,
} from '@/features/events/components/host';
import { Gallery } from '@/features/events/components/gallery';
import type { LinkState } from '@/features/people/invitations';
import { listInvites, openLinkState, photosOf } from '@/features/events/queries';
import { readableWindow, toWallClock, zoneLabel } from '@/features/events/time';
import { encodeId, tryDecodeId } from '@/lib/ids';
import { publicOrigin } from '@/lib/env';
import { userStreamPath } from '@/lib/paths';
import { requireSubjectPerson } from '@/lib/tiers';

export const metadata: Metadata = { title: 'Event' };

const STATE_WORD: Record<LinkState, string> = {
  live: 'Not opened',
  opened: 'Opened',
  claimed: 'Claimed',
  revoked: 'Revoked',
  expired: 'Expired',
};

const ANSWER_WORD = { yes: 'Yes', maybe: 'Maybe', no: 'No' } as const;

export default async function EventPage({
  params,
}: {
  params: Promise<{ user: string; id: string }>;
}) {
  const { user, id } = await params;
  const { reader } = await requireSubjectPerson(user, `events/${id}`);
  const internal = tryDecodeId('event', id);
  if (internal === null) notFound();

  const event = await database().transaction((tx) => hostedEvent(tx, reader, internal));
  if (!event) notFound();

  const invites = await listInvites(event.id);
  /* The host sees the pictures they have taken down as well as the ones on
     the page: the control that puts one back has to show which one it is. */
  const photos = await photosOf(event.id, { hidden: true });
  const open = await openLinkState(event.id);
  const counts = headcount(
    invites
      .filter((row) => row.response !== null)
      .map((row) => ({ response: row.response!, plusOne: row.plusOne })),
  );
  const past = overflow(event.capacity, invites
    .filter((row) => row.response !== null)
    .map((row) => ({ response: row.response!, plusOne: row.plusOne })));

  return (
    <main className="indoors">
      {/* The guest list is the site's first live surface: a guest answering on
          their own phone moves the numbers and the rows here without anybody
          reloading. It listens on the host's own stream, filtered to rsvps,
          and answers an event by refetching this page through the same
          authorized read path that drew it. */}
      <GuestListStream stream={userStreamPath(user)} />
      <h1>{event.title}</h1>
      <p className="note">
        {readableWindow(event.startsAt, event.endsAt, event.timezone)}{' '}
        {zoneLabel(event.startsAt, event.timezone)} ·{' '}
        <span className="state" data-state={event.publishedAt ? 'claimed' : 'live'}>
          {event.publishedAt ? 'Published' : 'Draft'}
        </span>
        {event.publishedAt ? (
          <>
            {' · '}
            <Link href={`/e/${event.slug}`}>
              {publicOrigin()}/e/{event.slug}
            </Link>
          </>
        ) : null}
      </p>

      <section>
        <h2>The page</h2>
        <EventForm
          draft={{
            id,
            title: event.title,
            startsAt: toWallClock(event.startsAt, event.timezone),
            endsAt: event.endsAt ? toWallClock(event.endsAt, event.timezone) : '',
            timezone: event.timezone,
            location: event.location ?? '',
            address: event.address ?? '',
            body: event.body,
            capacity: event.capacity === null ? '' : String(event.capacity),
            colour: event.colour ?? '',
            posterUrl: event.posterUrl ?? '',
            revealGuests: event.revealGuests,
          }}
        />
      </section>

      <section>
        <h2>Publication</h2>
        <p className="note">
          {event.publishedAt
            ? 'Changes after publishing simply update the page. Nobody is mailed; calendars follow.'
            : 'Publishing mints one link per invited person and one open link, shown once.'}
        </p>
        <PublishButton event={id} published={event.publishedAt !== null} />
        {/* A div rather than a p: `LinkButton` is a form, and a form inside a
            paragraph is closed by the parser before it is parsed — the markup
            the browser builds is not the markup React rendered. */}
        {event.publishedAt ? (
          <div className="aside-line">
            Open link: <span className="state" data-state={open ?? 'live'}>{open ? STATE_WORD[open] : 'None'}</span>{' '}
            <LinkButton event={id} label="New open link" />
          </div>
        ) : null}
      </section>

      <section>
        <InviteForm event={id} />
      </section>

      <section>
        <h2>
          Responses — {counts.yes} yes, {counts.maybe} maybe, {counts.no} no,{' '}
          {counts.coming} coming
          {event.capacity === null ? null : ` of ${event.capacity}`}
          {past > 0 ? `, ${past} past the line` : null}
        </h2>
        <div className="scroller">
          <table className="rows wide">
            <thead>
              <tr>
                <th>Guest</th>
                <th>Handle</th>
                <th>Link</th>
                <th>Answer</th>
                <th>Note</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {invites.map((row) => (
                <tr key={row.inviteId}>
                  <td>{row.displayName}</td>
                  <td className="mono">{row.handle ?? '—'}</td>
                  <td>
                    {row.link ? (
                      <span className="state" data-state={row.link.state}>
                        {STATE_WORD[row.link.state]}
                      </span>
                    ) : (
                      <span className="note">None</span>
                    )}
                  </td>
                  <td>
                    {row.response ? (
                      <span className="state" data-state={row.response === 'yes' ? 'claimed' : row.response === 'maybe' ? 'opened' : 'revoked'}>
                        {ANSWER_WORD[row.response]}
                        {row.plusOne > 0 ? ` +${row.plusOne}` : null}
                      </span>
                    ) : (
                      <span className="note">—</span>
                    )}
                  </td>
                  <td className="wrap">{row.note ?? ''}</td>
                  <td className="actions">
                    <LinkButton event={id} person={encodeId('person', row.personId)} label="New link" />
                    <RemoveInviteButton event={id} person={encodeId('person', row.personId)} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {invites.length === 0 ? (
          <p className="empty">Nobody invited yet. The open link works without a list.</p>
        ) : null}
      </section>

      {/* Guests add these from the event page once the evening has started.
          Hiding one is reversible and is recorded: the row stays, the picture
          stops being named in anybody's HTML. */}
      <section>
        <Gallery slug={event.slug} photos={photos} host={{ event: id }} />
        {photos.length === 0 ? (
          <>
            <h2>Pictures</h2>
            <p className="empty">
              Nothing yet. Guests who said yes can add pictures once the evening has started.
            </p>
          </>
        ) : null}
      </section>
    </main>
  );
}
