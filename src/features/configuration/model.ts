import { desc, eq, and } from 'drizzle-orm';
import { z } from 'zod';
import { database, schema } from '@/db/client';
import { readDraft, readPublished } from '@/lib/fleet/configured';

export const HOME_KEY = 'homepage';
export const ANNOUNCEMENT_KEY = 'announcement';
export const homeSchema = z.object({
  heading: z.string().min(1).max(120),
  tagline: z.string().min(1).max(240),
});
export const announcementSchema = z.object({ enabled: z.boolean(), text: z.string().max(300) });
export const HOME_DEFAULT = { heading: 'Ronit Nath', tagline: 'Founder of Isoastra' };
export const ANNOUNCEMENT_DEFAULT = { enabled: false, text: '' };
export const configurationDefinitions = [
  {
    key: HOME_KEY,
    policy: 'draft/published' as const,
    scope: 'public homepage',
    dependencies: ['/', 'homepage hero'],
    schema: homeSchema,
  },
  {
    key: ANNOUNCEMENT_KEY,
    policy: 'live' as const,
    scope: 'public homepage',
    dependencies: ['/', 'announcement banner'],
    schema: announcementSchema,
  },
] as const;

export async function publicConfiguration() {
  const db = database();
  const [home, announcement] = await Promise.all([
    readPublished(db, 'isoastra', HOME_KEY, homeSchema),
    readPublished(db, 'isoastra', ANNOUNCEMENT_KEY, announcementSchema),
  ]);
  return { home: home ?? HOME_DEFAULT, announcement: announcement ?? ANNOUNCEMENT_DEFAULT };
}

export async function configurationState() {
  const db = database();
  const [home, homeDraft, announcement, announcementDraft, history] = await Promise.all([
    readPublished(db, 'isoastra', HOME_KEY, homeSchema),
    readDraft(db, 'isoastra', HOME_KEY),
    readPublished(db, 'isoastra', ANNOUNCEMENT_KEY, announcementSchema),
    readDraft(db, 'isoastra', ANNOUNCEMENT_KEY),
    db
      .select()
      .from(schema.configuredVersion)
      .where(eq(schema.configuredVersion.orgId, 'isoastra'))
      .orderBy(desc(schema.configuredVersion.at))
      .limit(50),
  ]);
  return {
    home: home ?? HOME_DEFAULT,
    homeDraft,
    announcement: announcement ?? ANNOUNCEMENT_DEFAULT,
    announcementDraft,
    history,
  };
}

export async function affectedSubscribers(key: string) {
  return database()
    .select({
      viewId: schema.realtimeSubscription.viewId,
      descriptor: schema.realtimeSubscription.descriptor,
    })
    .from(schema.realtimeSubscription)
    .innerJoin(
      schema.realtimeView,
      eq(schema.realtimeView.id, schema.realtimeSubscription.viewId),
    )
    .where(
      and(
        eq(
          schema.realtimeSubscription.subscriptionKey,
          `configuration:${key}:${key === ANNOUNCEMENT_KEY ? 'live' : 'published'}`,
        ),
      ),
    );
}
