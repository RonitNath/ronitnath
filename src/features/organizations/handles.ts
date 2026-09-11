/* An organization's handle is its address: `/o/<handle>`. It is chosen once
 * and it shares the URL space with the site's own top-level paths and with the
 * one name Isoastra keeps for itself under `/o`, so the reserved list is part
 * of the validation rather than a thing to remember. */

export const ORG_HANDLE_MIN = 2;
export const ORG_HANDLE_MAX = 32;

/* Every first path segment this deployment already answers on, plus the one
 * segment under `/o` that is not an organization: `isoastra` is the operator
 * console, a static route that would silently shadow any organization that
 * took the name. */
const RESERVED = new Set([
  'app',
  'api',
  'auth',
  'd',
  'e',
  'healthz',
  'isoastra',
  'links',
  'o',
  'org',
  'platform',
  'public',
  'static',
  'u',
  '_next',
]);

/** The handle as it will be stored, or null when what was typed is not one. */
export function normalizeOrgHandle(raw: string): string | null {
  const handle = raw.trim().toLowerCase();
  if (handle.length < ORG_HANDLE_MIN || handle.length > ORG_HANDLE_MAX) return null;
  if (!/^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/.test(handle)) return null;
  if (handle.includes('--')) return null;
  if (RESERVED.has(handle)) return null;
  return handle;
}
