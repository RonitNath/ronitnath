/* The URL a request arrived at, as the public sees it.
 *
 * Behind the edge the process is bound to 0.0.0.0:3140 and Next reports that
 * bind address in `request.url`, so anything derived from it — a redirect, or
 * the `redirect_uri` openid-client rebuilds for the token exchange — names a
 * host nobody can reach. Every absolute URL is therefore built from
 * `PUBLIC_ORIGIN` plus the request's own path and query. */

import { publicOrigin } from './env';

export function publicRequestUrl(request: Request): URL {
  const incoming = new URL(request.url);
  return new URL(`${incoming.pathname}${incoming.search}`, publicOrigin());
}

export function publicUrl(pathAndQuery: string): URL {
  return new URL(pathAndQuery, publicOrigin());
}
