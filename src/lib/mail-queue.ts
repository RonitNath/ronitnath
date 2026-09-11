import { requestContext } from '@/lib/fleet/context';
import { persistMailIntent, type MailIntentKind } from '@/lib/mail-intents';
import type { Message } from '@/lib/mail';

export async function queueMailIntent(kind: MailIntentKind, message: Message, expiresAt: Date): Promise<string> {
  const context = await requestContext();
  return persistMailIntent(kind, message, expiresAt, context.correlationId);
}
