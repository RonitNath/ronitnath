import { requestContext } from '@/lib/fleet/context';
import { persistAuthDeliveryIntent, type AuthMailKind } from '@/lib/notification-delivery';
import type { Message } from '@/lib/mail';

export async function queueMailIntent(kind: AuthMailKind, message: Message, expiresAt: Date): Promise<string> {
  const context = await requestContext();
  return persistAuthDeliveryIntent(kind, message, expiresAt, context.correlationId);
}
