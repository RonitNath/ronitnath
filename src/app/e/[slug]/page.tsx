import type { Metadata } from 'next';
import Link from 'next/link';

import { database, schema } from '@/db/client';
import { publishedEvent } from '@/features/events/authority';
import { AnswerForm, LocalTime } from '@/features/events/components/guest';
import { guestView } from '@/features/events/guest';
import { readEventLink } from '@/features/events/links';
import { readableWindow, zoneLabel } from '@/features/events/time';
import { currentPrincipal } from '@/features/auth/principal';
import { ThemeToggle } from '@/app/theme-toggle';
import { and, eq } from 'drizzle-orm';

import '../../auth/auth.css';
import './guest.css';

export const metadata: Metadata = { title: 'An invitation' };
export const dynamic = 'force-dynamic';

/* Unpublished, revoked, expired, never existed: one page, one sentence. A
 * visitor who can tell those apart can use this URL to learn things about
 * people they were never handed a link for. */
function Declined() {
  return (
    <main className="door single">
      <h1>Ronit Nath</h1>
      <p className="note">This link does not work.</p>
      <div className="aside">
        <Link href="/">Back to the landing</Link>
      </div>
    </main>
  );
}

export default async function GuestPage({
  params,
  searchParams,
}: {
  params: Promise<{ slug: string }>;
  searchParams: Promise<{ l?: string }>;
}) {
  const { slug } = await params;
  const { l: token = '' } = await searchParams;
  const principal = await currentPrincipal();

  const seen = await database().transaction(async (tx) => {
    const event = await publishedEvent(tx, slug);
    if (!event) return null;
    const link = token ? await readEventLink(tx, token) : null;
    if (token && !link) return null;
    if (link?.kind === 'event_open' && link.targetId !== event.id) return null;

    /* A personal link names the guest; it opens this page only if that guest
     * was asked to this event. */
    let viewer: { personId: number | null; name: string | null; plusOneAllowed: boolean } | null =
      null;
    if (link?.kind === 'event_personal') {
      const rows = await tx
        .select({
          displayName: schema.person.displayName,
          plusOneAllowed: schema.eventInvite.plusOneAllowed,
        })
        .from(schema.eventInvite)
        .innerJoin(schema.person, eq(schema.person.id, schema.eventInvite.personId))
        .where(
          and(
            eq(schema.eventInvite.eventId, event.id),
            eq(schema.eventInvite.personId, link.targetId),
          ),
        )
        .limit(1);
      const row = rows[0];
      if (!row) return null;
      viewer = { personId: link.targetId, name: row.displayName, plusOneAllowed: row.plusOneAllowed };
    } else if (link?.kind === 'event_open') {
      viewer = { personId: null, name: null, plusOneAllowed: true };
    } else if (principal) {
      const rows = await tx
        .select({ plusOneAllowed: schema.eventInvite.plusOneAllowed })
        .from(schema.eventInvite)
        .where(
          and(
            eq(schema.eventInvite.eventId, event.id),
            eq(schema.eventInvite.personId, principal.personId),
          ),
        )
        .limit(1);
      if (rows.length === 0 && event.hostPersonId !== principal.personId) return null;
      viewer = {
        personId: principal.personId,
        name: principal.displayName,
        plusOneAllowed: rows[0]?.plusOneAllowed ?? true,
      };
    } else {
      return null;
    }
    return { event, viewer, open: link?.kind === 'event_open' };
  });

  const header = (
    <header className="topbar">
      <ThemeToggle />
    </header>
  );
  if (!seen) {
    return (
      <>
        {header}
        <Declined />
      </>
    );
  }

  const view = await guestView(seen.event, seen.viewer);
  const answer = view.viewer?.answer ?? null;
  const said = answer?.response ?? null;
  const server = `${readableWindow(view.startsAt, view.endsAt, view.timezone)} ${zoneLabel(view.startsAt, view.timezone)}`;

  return (
    <>
      {header}
      <main className="door invite" data-colour={view.colour ?? undefined}>
        {view.posterUrl ? (
          /* eslint-disable-next-line @next/next/no-img-element */
          <img className="poster" src={view.posterUrl} alt="" />
        ) : null}
        <h1>{view.title}</h1>
        <p className="lede">
          <LocalTime
            startsAt={view.startsAt.toISOString()}
            endsAt={view.endsAt?.toISOString() ?? null}
            timezone={view.timezone}
            server={server}
          />
          {view.location ? <> · {view.location}</> : null}
          <br />
          <span className="host">{view.hostName} is hosting</span>
          {view.viewer?.name ? <> · {view.viewer.name}</> : null}
        </p>

        {view.bodyHtml ? (
          /* Markdown-lite, escaped and rebuilt by src/features/events/markup.ts:
           * the only tags here are the ones that file can emit. */
          <div className="body" dangerouslySetInnerHTML={{ __html: view.bodyHtml }} />
        ) : null}

        {said === 'yes' && view.address ? (
          <section className="details">
            <h2>Where</h2>
            <p>{view.address}</p>
            <p className="aside-line">
              <a href={`/e/${view.slug}/calendar.ics${token ? `?l=${token}` : ''}`}>
                Add to calendar
              </a>
            </p>
          </section>
        ) : null}
        {said === 'yes' && !view.address ? (
          <p className="aside-line">
            <a href={`/e/${view.slug}/calendar.ics${token ? `?l=${token}` : ''}`}>Add to calendar</a>
          </p>
        ) : null}

        <section className="who-list" data-blurred={view.blurred}>
          <h2>
            Who&rsquo;s coming
            {view.room === 'full' ? <span className="full"> · full</span> : null}
          </h2>
          {said === 'yes' && view.room === 'full' ? (
            <p className="note">The room is full. You are on the list past the line.</p>
          ) : null}
          {view.guests.length === 0 ? (
            <p className="note">Nobody has answered yet. You can be first.</p>
          ) : (
            <ul className="names">
              {view.guests.map((guest, index) => (
                <li key={index} className="name" data-shared={guest.shared}>
                  {guest.label}
                  {guest.plusOne > 0 ? ` +${guest.plusOne}` : null}
                </li>
              ))}
              {view.more > 0 ? <li className="more">and {view.more} more</li> : null}
            </ul>
          )}
        </section>

        <AnswerForm
          slug={view.slug}
          token={token}
          answer={said}
          plusOne={answer?.plusOne ?? 0}
          note={answer?.note ?? ''}
          needsName={seen.open}
          plusOneAllowed={view.plusOneAllowed}
        />

        <div className="aside">
          <span>No account needed. Come back to this link to change your answer.</span>
        </div>
      </main>
    </>
  );
}
