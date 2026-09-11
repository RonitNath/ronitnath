/* The stub that keeps every `/app/...` link ever pasted somewhere working.
 *
 * `/app` was an audience route: it named who you were rather than what you
 * were looking at, and the grammar has no room for one (fleet-conventions
 * §1). Its replacement is `/u/<user>/...`, where `<user>` is the reader's own
 * public id — which is exactly why the old paths cannot be retired with a
 * rewrite in `next.config.ts`. A static rewrite knows the path and nothing
 * else; the destination here depends on who is holding the cookie, so the
 * redirect has to be resolved by something that can read a session. That is
 * this route segment.
 *
 * The work is split in two because of what each file can see. This layout is
 * the segment's declaration — it is dynamic, it has no chrome of its own, and
 * everything under it is a redirect rather than a page. The catch-all page
 * beside it is the half that receives the remaining path segments, which a
 * layout is never given, and it is where the session is resolved and the
 * permanent redirect issued. */

export const dynamic = 'force-dynamic';

export default function LegacyAppLayout({ children }: { children: React.ReactNode }) {
  return children;
}
