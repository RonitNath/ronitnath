/* The W0 export, folded back into this site's own tables.
 *
 * Between 2026-09-05 and 2026-09-10 ronitnath.com was a tenant of a Payload
 * installation, and whatever was written there — one event, as it turns out —
 * exists nowhere else. `export/ronitnath/*.json` at Isoastra/Isoastra@97745bad
 * is the record of it, and this reads that directory into `person`, `event`,
 * `event_invite`, `rsvp` and `audit`.
 *
 * Idempotent, on purpose and by test: every row is keyed on the public
 * identifier it already had — an event by its slug, a person by their address,
 * an rsvp by the pair it belongs to — so running it twice changes nothing and
 * running it after somebody has edited an event does not undo the edit. It is
 * an import, not a restore.
 *
 * Usage: `pnpm import:export --from <dir> [--host <email>] [--dry-run]`.
 * `--host` names the person a Payload event is attached to here; Payload's
 * events had a tenant rather than a host, and an event on this site is
 * somebody's. It defaults to the site's operator. */

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { and, eq, isNull, sql } from 'drizzle-orm';

import { database, schema } from '@/db/client';

interface ExportedEvent {
  id: number;
  title: string;
  slug: string;
  startsAt: string;
  endsAt: string | null;
  timeZone: string | null;
  placeName: string | null;
  placeApproximation: string | null;
  address: string | null;
  body: string | null;
  state: string | null;
  revealGuests: boolean | null;
  createdAt: string;
  updatedAt: string;
}

interface ExportedPerson {
  id: number;
  name?: string | null;
  email?: string | null;
  createdAt?: string;
}

interface ExportedRsvp {
  id: number;
  event?: { slug?: string } | number | null;
  person?: { email?: string } | number | null;
  response?: string | null;
  plusOne?: number | null;
  note?: string | null;
  createdAt?: string;
}

interface ExportedAudit {
  id: number;
  at: string;
  command: string;
  actorKind?: string | null;
  actorId?: string | null;
  targetCollection?: string | null;
  targetId?: string | null;
  payload?: unknown;
}

function read<T>(dir: string, name: string): T[] {
  try {
    const parsed: unknown = JSON.parse(readFileSync(join(dir, name), 'utf8'));
    return Array.isArray(parsed) ? (parsed as T[]) : [];
  } catch {
    return [];
  }
}

function flag(name: string): string | undefined {
  const at = process.argv.indexOf(`--${name}`);
  return at === -1 ? undefined : process.argv[at + 1];
}

/* Payload's `placeName` is the name a guest may read before answering and its
 * `placeApproximation` is the neighbourhood; this site calls the first
 * `location` and has no column for the second, so it goes where a host would
 * have typed it — into the body, above whatever else is there. Losing it would
 * be losing the only hint an unanswered guest has about where they are going. */
function bodyOf(row: ExportedEvent): string {
  const parts = [row.body?.trim(), row.placeApproximation?.trim()].filter(
    (part): part is string => Boolean(part),
  );
  return parts.join('\n\n');
}

async function main(): Promise<void> {
  const dir = flag('from');
  if (!dir) throw new Error('--from <dir> is required (the export/ronitnath directory)');
  const dryRun = process.argv.includes('--dry-run');
  const db = database();

  const events = read<ExportedEvent>(dir, 'events.json');
  const people = read<ExportedPerson>(dir, 'people.json');
  const rsvps = read<ExportedRsvp>(dir, 'rsvps.json');
  const audits = read<ExportedAudit>(dir, 'audit.json');
  const photos = read<unknown>(dir, 'photos.json');

  const counted = { people: 0, events: 0, rsvps: 0, audit: 0 };

  await db.transaction(async (tx) => {
    /* The host. Every imported event belongs to somebody, and on this site
     * that is a person row rather than a tenant. */
    const hostEmail = flag('host');
    const host = await resolveHost(tx, hostEmail);

    /* People first: an rsvp names one, and a guest exported from Payload has
     * never signed in here, so they arrive `held` — which is exactly what this
     * site already means by somebody a host named but who has no door. */
    const personByEmail = new Map<string, number>();
    for (const row of people) {
      const email = row.email?.trim().toLowerCase();
      if (!email) continue;
      const existing = await tx
        .select({ personId: schema.identity.personId })
        .from(schema.identity)
        .where(eq(schema.identity.subject, email))
        .limit(1);
      if (existing[0]) {
        personByEmail.set(email, existing[0].personId);
        continue;
      }
      if (dryRun) continue;
      const parties = await tx
        .insert(schema.party)
        .values({ kind: 'person' })
        .returning({ id: schema.party.id });
      const id = parties[0]!.id;
      await tx
        .insert(schema.person)
        .values({ id, displayName: row.name?.trim() || email, held: true });
      await tx
        .insert(schema.identity)
        .values({ personId: id, source: 'handle', subject: email })
        .onConflictDoNothing();
      personByEmail.set(email, id);
      counted.people += 1;
    }

    const eventBySlug = new Map<string, number>();
    for (const row of events) {
      const existing = await tx
        .select({ id: schema.event.id })
        .from(schema.event)
        .where(eq(schema.event.slug, row.slug))
        .limit(1);
      if (existing[0]) {
        eventBySlug.set(row.slug, existing[0].id);
        continue;
      }
      if (dryRun) continue;
      const inserted = await tx
        .insert(schema.event)
        .values({
          hostPersonId: host,
          slug: row.slug,
          title: row.title,
          startsAt: new Date(row.startsAt),
          endsAt: row.endsAt ? new Date(row.endsAt) : null,
          timezone: row.timeZone ?? 'America/Los_Angeles',
          location: row.placeName,
          address: row.address,
          body: bodyOf(row),
          revealGuests: row.revealGuests ?? false,
          /* Payload's `state` was `draft` or `open`; an open event on this
           * site is one with a publication instant, and the export carries the
           * moment it was written. */
          publishedAt: row.state === 'open' ? new Date(row.createdAt) : null,
          createdAt: new Date(row.createdAt),
          updatedAt: new Date(row.updatedAt),
        })
        .returning({ id: schema.event.id });
      eventBySlug.set(row.slug, inserted[0]!.id);
      counted.events += 1;
    }

    for (const row of rsvps) {
      const slug = typeof row.event === 'object' && row.event ? row.event.slug : undefined;
      const email =
        typeof row.person === 'object' && row.person
          ? row.person.email?.trim().toLowerCase()
          : undefined;
      if (!slug || !email) continue;
      const eventId = eventBySlug.get(slug);
      const personId = personByEmail.get(email);
      if (eventId === undefined || personId === undefined || dryRun) continue;
      const response = row.response === 'yes' || row.response === 'no' ? row.response : 'maybe';
      await tx
        .insert(schema.rsvp)
        .values({
          eventId,
          personId,
          response,
          plusOne: row.plusOne ?? 0,
          note: row.note ?? null,
          answeredAt: row.createdAt ? new Date(row.createdAt) : new Date(),
        })
        .onConflictDoNothing();
      counted.rsvps += 1;
    }

    /* The audit rows are the record of what the Payload installation did on
     * this site's behalf. They come across as this site's own audit rows with
     * no actor — nobody here did them — and the Payload identifiers ride in
     * the payload so the trail can still be followed back. */
    for (const row of audits) {
      const command = `payload:${row.command}`;
      const at = new Date(row.at);
      const already = await tx
        .select({ id: schema.audit.id })
        .from(schema.audit)
        .where(
          and(
            eq(schema.audit.command, command),
            eq(schema.audit.at, at),
            isNull(schema.audit.actorPersonId),
          ),
        )
        .limit(1);
      if (already[0] || dryRun) continue;
      await tx.insert(schema.audit).values({
        actorPersonId: null,
        command,
        targetKind: row.targetCollection ?? null,
        targetId: null,
        payload: {
          source: 'payload-export',
          exportId: row.id,
          actorKind: row.actorKind ?? null,
          actorId: row.actorId ?? null,
          targetId: row.targetId ?? null,
          body: row.payload ?? null,
        },
        at,
      });
      counted.audit += 1;
    }
  });

  console.log(
    `import-export: ${counted.people} people, ${counted.events} events, ${counted.rsvps} rsvps, ${counted.audit} audit rows written` +
      (photos.length > 0 ? `; ${photos.length} photos in the export were not imported` : '') +
      (dryRun ? ' (dry run: nothing written)' : ''),
  );
}

/* Whose events these become. Named by address if the caller says so, otherwise
 * the operator — the person holding `operator` on `platform:0`, which on this
 * site is one row and one human. */
async function resolveHost(
  tx: Parameters<Parameters<ReturnType<typeof database>['transaction']>[0]>[0],
  email: string | undefined,
): Promise<number> {
  if (email) {
    const rows = await tx
      .select({ personId: schema.identity.personId })
      .from(schema.identity)
      .where(eq(schema.identity.subject, email.trim().toLowerCase()))
      .limit(1);
    if (!rows[0]) throw new Error(`no person here has the address ${email}`);
    return rows[0].personId;
  }
  const rows = await tx
    .select({ personId: schema.relation.subjectId })
    .from(schema.relation)
    .where(
      and(
        eq(schema.relation.subjectKind, 'person'),
        eq(schema.relation.verb, 'operator'),
        eq(schema.relation.resourceKind, 'platform'),
      ),
    )
    .orderBy(sql`${schema.relation.id} asc`)
    .limit(1);
  if (!rows[0]) {
    throw new Error('no operator exists yet: seed one, or pass --host <email>');
  }
  return rows[0].personId;
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});
