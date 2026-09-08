/** A token bucket per caller, in this process's memory.
 *
 * Deliberately not in Postgres. `auth_throttle` is a *security* counter — it
 * has to survive a restart and be the same count on every node, because it
 * stands between an attacker and a password. This is not that: it stands
 * between a scraper and a read-only catalogue that is already served with a
 * day of `max-age`, and the worst a lost bucket costs is a few extra index
 * lookups. A row write per read would be the more expensive thing.
 *
 * Buckets are swept lazily: a bucket that has been full for a whole refill
 * period is indistinguishable from one that never existed, so it is dropped
 * rather than kept for a caller who may never come back.
 */

interface Bucket {
  tokens: number;
  at: number;
}

export interface Limiter {
  /** Spend one token. `false` means the caller is over their rate. */
  take(key: string): boolean;
  readonly size: number;
}

/** `capacity` tokens, refilled at `capacity / windowMs`. A burst of
 * `capacity` is allowed — a panel opening fires one request and a visitor
 * clicking around fires a handful — and the sustained rate is the window. */
export function tokenBucket(
  capacity: number,
  windowMs: number,
  now: () => number = Date.now,
): Limiter {
  const buckets = new Map<string, Bucket>();
  const perMs = capacity / windowMs;
  return {
    take(key: string): boolean {
      const at = now();
      const bucket = buckets.get(key);
      let tokens = capacity;
      if (bucket) tokens = Math.min(capacity, bucket.tokens + (at - bucket.at) * perMs);
      if (tokens < 1) {
        buckets.set(key, { tokens, at });
        return false;
      }
      buckets.set(key, { tokens: tokens - 1, at });
      if (buckets.size > 1_000) sweep(buckets, at, windowMs);
      return true;
    },
    get size(): number {
      return buckets.size;
    },
  };
}

function sweep(buckets: Map<string, Bucket>, at: number, windowMs: number): void {
  for (const [key, bucket] of buckets) {
    if (at - bucket.at > windowMs) buckets.delete(key);
  }
}

/** Who is asking, behind the edge. The proxy appends the real address to
 * `x-forwarded-for`, so the *first* entry is the client and everything after
 * it is a hop; a request that arrives with neither header is one machine
 * talking to itself and shares a bucket. */
export function callerKey(headers: Headers): string {
  const forwarded = headers.get('x-forwarded-for');
  if (forwarded) {
    const first = forwarded.split(',')[0]?.trim();
    if (first) return first;
  }
  return headers.get('x-real-ip')?.trim() || 'local';
}
