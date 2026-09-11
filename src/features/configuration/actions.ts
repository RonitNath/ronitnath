'use server';

import { z } from 'zod';
import { revalidatePath } from 'next/cache';
import { database } from '@/db/client';
import { requireOperator } from '@/lib/tiers';
import { ConfiguredConflict, publish, saveDraft } from '@/lib/fleet/configured';
import { ANNOUNCEMENT_KEY, announcementSchema, HOME_KEY, homeSchema } from './model';

export type ConfigurationResult = { notice?: string; error?: string };
const version = z.coerce.number().int().nonnegative();
const value = (form: FormData, key: string) => String(form.get(key) ?? '');

export async function saveHome(
  _previous: ConfigurationResult,
  form: FormData,
): Promise<ConfigurationResult> {
  await requireOperator();
  const parsed = homeSchema.safeParse({
    heading: value(form, 'heading'),
    tagline: value(form, 'tagline'),
  });
  const expected = version.safeParse(form.get('version'));
  if (!parsed.success || !expected.success) return { error: 'Enter a heading and tagline.' };
  try {
    const next = await database().transaction((tx) =>
      saveDraft(tx, 'isoastra', HOME_KEY, parsed.data, expected.data),
    );
    revalidatePath('/o/isoastra/configuration');
    return { notice: `Draft ${next} saved.` };
  } catch (error) {
    return {
      error:
        error instanceof ConfiguredConflict
          ? 'The draft changed in another window. Reload and review it.'
          : 'The draft could not be saved.',
    };
  }
}

export async function publishHome(
  _previous: ConfigurationResult,
  form: FormData,
): Promise<ConfigurationResult> {
  await requireOperator();
  const expected = version.safeParse(form.get('version'));
  if (!expected.success) return { error: 'Reload the draft before publishing.' };
  try {
    const live = await database().transaction((tx) =>
      publish(tx, 'isoastra', HOME_KEY, expected.data),
    );
    revalidatePath('/');
    revalidatePath('/o/isoastra/configuration');
    return { notice: `Published revision ${live}.` };
  } catch {
    return { error: 'The draft moved. Reload and review it.' };
  }
}

export async function saveAnnouncement(
  _previous: ConfigurationResult,
  form: FormData,
): Promise<ConfigurationResult> {
  await requireOperator();
  const parsed = announcementSchema.safeParse({
    enabled: form.get('enabled') === 'on',
    text: value(form, 'text'),
  });
  const expected = version.safeParse(form.get('version'));
  if (!parsed.success || !expected.success) return { error: 'The announcement is invalid.' };
  try {
    const live = await database().transaction(async (tx) => {
      const next = await saveDraft(
        tx,
        'isoastra',
        ANNOUNCEMENT_KEY,
        parsed.data,
        expected.data,
      );
      await publish(tx, 'isoastra', ANNOUNCEMENT_KEY, next);
      return next;
    });
    revalidatePath('/');
    revalidatePath('/o/isoastra/configuration');
    return { notice: `Live revision ${live}.` };
  } catch {
    return { error: 'The announcement changed in another window. Reload and review it.' };
  }
}
