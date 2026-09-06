import type { Metadata } from 'next';

import { EventForm } from '@/features/events/components/host';
import { toWallClock } from '@/features/events/time';
import { requireMember } from '@/lib/tiers';

export const metadata: Metadata = { title: 'New event' };

const DEFAULT_ZONE = 'America/Los_Angeles';

export default async function NewEventPage() {
  await requireMember('/app/events/new');
  /* A sensible first guess rather than an empty field: the evening after
   * next, at seven, in the deployment's zone. */
  const soon = new Date(Date.now() + 2 * 86_400_000);
  soon.setUTCMinutes(0, 0, 0);

  return (
    <main className="indoors">
      <h1>New event</h1>
      <p className="note">
        Created is not published. Nothing outside this page can see it until you publish, and
        publishing is what mints the links.
      </p>
      <section>
        <EventForm
          draft={{
            title: '',
            startsAt: toWallClock(soon, DEFAULT_ZONE),
            endsAt: '',
            timezone: DEFAULT_ZONE,
            location: '',
            address: '',
            body: '',
            capacity: '',
            colour: '',
            posterUrl: '',
            revealGuests: false,
          }}
        />
      </section>
    </main>
  );
}
