/* Addresses, normalised once so that the unique index on
 * identity(source, subject) is the thing that decides who is who.
 *
 * Normalisation is case-folding and trimming only. The local part is
 * case-sensitive by RFC and dot-folding is a Gmail policy, not a mail one;
 * folding either would merge two people who are not the same person. */

export function normalizeEmail(raw: string): string {
  return raw.trim().toLowerCase();
}

export function looksLikeEmail(value: string): boolean {
  return /^[^\s@]+@[^\s@.]+(\.[^\s@.]+)+$/.test(value) && value.length <= 254;
}

/* The allowlist is the OIDC door's whole guest list. Ronit signs in through
 * ZITADEL and never holds a password, so the same list is what Register and
 * ResetPassword refuse — uniformly, saying nothing about who is on it. */
export function oidcAllowlist(raw = process.env.OIDC_ALLOWLIST): string[] {
  return (raw ?? '')
    .split(/[,\s]+/)
    .map(normalizeEmail)
    .filter((entry) => entry.length > 0);
}

export function isAllowlisted(email: string, raw?: string): boolean {
  return oidcAllowlist(raw).includes(normalizeEmail(email));
}
