/* Environment, read once and named once. A missing value that only a
 * deployment can supply throws where it is used, never at import: the build
 * loads every route module and a route module must not need a secret to
 * exist. */

export function required(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is not set`);
  return value;
}

export function publicOrigin(): string {
  return process.env.PUBLIC_ORIGIN?.replace(/\/+$/, '') ?? 'http://localhost:3000';
}

export function sessionCookieName(): string {
  return process.env.SESSION_COOKIE ?? 'rn_session';
}

export function sessionTtlDays(): number {
  const parsed = Number(process.env.SESSION_TTL_DAYS ?? 30);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 30;
}

export function isProduction(): boolean {
  return process.env.NODE_ENV === 'production';
}
