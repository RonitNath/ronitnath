/* Machine-given values, said the same way on every operator page: a stamp to
 * the minute, a word for a state, and JSON a person can read. */

export function stamp(at: Date): string {
  return at.toISOString().slice(0, 16).replace('T', ' ');
}

export function day(at: Date): string {
  return at.toISOString().slice(0, 10);
}

/** A user agent is a paragraph nobody reads; this is the half a person
 *  recognises, and the whole string stays on the row's title. */
export function agentOf(raw: string | null): string {
  if (!raw) return 'Unknown';
  const browser = /Firefox\//.test(raw)
    ? 'Firefox'
    : /Edg\//.test(raw)
      ? 'Edge'
      : /Chrome\//.test(raw)
        ? 'Chrome'
        : /Safari\//.test(raw)
          ? 'Safari'
          : 'Browser';
  const platform = /Android/.test(raw)
    ? 'Android'
    : /iPhone|iPad/.test(raw)
      ? 'iOS'
      : /Mac OS X/.test(raw)
        ? 'macOS'
        : /Windows/.test(raw)
          ? 'Windows'
          : /Linux/.test(raw)
            ? 'Linux'
            : '';
  return platform ? `${browser} on ${platform}` : browser;
}

export function pretty(payload: unknown): string {
  if (payload === null || payload === undefined) return '';
  return JSON.stringify(payload, null, 2);
}
