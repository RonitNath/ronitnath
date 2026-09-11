/* The URL grammar, in one place (fleet-conventions §1).
 *
 * The path says which object a page is about; the query says how it is shown.
 * A person's own surfaces live under `/u/<user>`, an organization's under
 * `/o/<org>`, and Isoastra's own console under `/o/isoastra` — there is no
 * `/app` and no `/admin` audience route any more, because an audience is not
 * an object and a link that names one cannot be shared with anybody who is
 * not in it.
 *
 * `<user>` is a person's opaque public id (`src/lib/ids.ts`, type `person`),
 * never an email and never an internal integer. `<org>` is an organization's
 * handle, which is already the name its members say out loud.
 *
 * These are string helpers and nothing else: no database, no session, no
 * crypto. That is deliberate — a client component may import this module, and
 * anything that reaches for `node:crypto` on the way in would break the build
 * rather than the page. A caller that has an internal id encodes it first.
 *
 * The `*_ROUTE` constants are the dynamic-segment forms Next wants when a
 * command revalidates every instance of a route rather than one reader's
 * (`revalidatePath(userRoute("events"), "page")`). A command that knows whose
 * page it changed should name that page instead; these are for the commands
 * that legitimately do not. */

/** The audience route this site used to answer on. It is not part of the
 *  grammar any more; it survives as one permanently-redirecting stub that
 *  resolves the session and forwards to `/u/<me>/…`, which is the one thing a
 *  static rewrite cannot do. Name it only where a path has to be handed out
 *  before anybody knows who "me" is — an OAuth callback URL, a bookmark. */
export const LEGACY_APP_ROOT = '/app';

/** Where Isoastra's own staff surfaces live. A static segment, so it wins
 *  over `/o/[org]` and no organization can take the name. */
export const OPERATOR_ROOT = '/o/isoastra';

/** The dynamic form of a person's root, for `revalidatePath(…, 'layout')`. */
export const USER_ROUTE = '/u/[user]';

function join(root: string, view: string): string {
  const trimmed = view.replace(/^\/+/, '');
  return trimmed === '' ? root : `${root}/${trimmed}`;
}

/** A person's surface. `user` is their public id; `view` is the rest of the
 *  path (`events`, `events/e_…`, `documents`). */
export function userPath(user: string, view = ''): string {
  return join(`/u/${user}`, view);
}

/** The dynamic form of one of a person's views, for a command that changed
 *  something every reader of that view can see. */
export function userRoute(view = ''): string {
  return join(USER_ROUTE, view);
}

/** An organization's surface, addressed by handle. */
export function orgPath(handle: string, view = ''): string {
  return join(`/o/${handle}`, view);
}

/** An Isoastra staff surface. */
export function operatorPath(view = ''): string {
  return join(OPERATOR_ROOT, view);
}

/** The stream a person's own events are published on, and the one a browser
 *  opens an `EventSource` against.
 *
 *  It is `…/events/stream` rather than `…/events` because on this site
 *  `/u/<user>/events` is already a page — the events somebody is hosting —
 *  and a Next route segment cannot be both. See the route handler's own
 *  comment for the whole decision. */
export function userStreamPath(user: string): string {
  return userPath(user, 'events/stream');
}

/** The stream an organization's events are published on. */
export function orgStreamPath(handle: string): string {
  return orgPath(handle, 'events');
}
