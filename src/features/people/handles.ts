/* Handles. What a member typed when they held somebody: an address, a phone
 * number, or just a name.
 *
 * The point of normalising is that a handle has to be *comparable* — the whole
 * of ProposeMatch is "this verified address equals that handle" — while what
 * the member sees stays what the member typed. So a handle row carries the
 * normalised form as its subject and the person carries the display name, and
 * the two are allowed to differ.
 *
 * Only an address normalises to something another door can equal. A phone
 * number keeps its digits and its country plus; a name is folded to lower case
 * and single spaces and matches nothing but another name, which is why a bare
 * name never proposes a match. */

import { looksLikeEmail, normalizeEmail } from '@/features/auth/email-address';

export type HandleKind = 'email' | 'phone' | 'name';

export interface Handle {
  kind: HandleKind;
  /* What goes in `identity.subject`. */
  subject: string;
  /* What the member typed, trimmed. */
  raw: string;
}

/* Enough digits that a house number or a birth year is not a phone number. */
const PHONE_SHAPE = /^\+?[\d\s().\-–—]{7,}$/;
const PHONE_MIN_DIGITS = 7;
const PHONE_MAX_DIGITS = 15;

export const HANDLE_MAX = 254;

function asPhone(raw: string): string | null {
  if (!PHONE_SHAPE.test(raw)) return null;
  const digits = raw.replace(/\D/g, '');
  if (digits.length < PHONE_MIN_DIGITS || digits.length > PHONE_MAX_DIGITS) return null;
  /* E.164 without the guessing: a leading + is kept because it is the one
   * thing that says the country code is already there, and nothing is
   * inferred when it is absent. */
  return raw.trimStart().startsWith('+') ? `+${digits}` : digits;
}

export function normalizeHandle(raw: string): Handle | null {
  const trimmed = raw.trim().replace(/\s+/g, ' ');
  if (trimmed.length === 0 || trimmed.length > HANDLE_MAX) return null;

  const address = normalizeEmail(trimmed);
  if (looksLikeEmail(address)) return { kind: 'email', subject: address, raw: trimmed };

  const phone = asPhone(trimmed);
  if (phone !== null) return { kind: 'phone', subject: phone, raw: trimmed };

  return { kind: 'name', subject: trimmed.toLowerCase(), raw: trimmed };
}

/** How a handle reads back to the member who typed it. */
export const HANDLE_LABEL: Record<HandleKind, string> = {
  email: 'Email',
  phone: 'Phone',
  name: 'Name',
};
