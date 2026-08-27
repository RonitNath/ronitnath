# Platform admin — user stories

Owner: Ronit Nath, as the operator of a deployment. Written for the ronitnath
deployment; each story says where the isoastra deployment differs. These feed
feature requirements; they are not bound to the current implementation.

Rulings that shape them (2026-08-27):

- One monolith codebase; every deployment (ronitnath.com, isoastra.com, a
  client's) is its own identity authority. No federation between deployments.
- Products are enabled per deployment at **runtime** — a platform screen, not a
  redeploy — because at many dozens of products and services this has to be
  fast and dynamic.
- The operator has **god mode**: full authority over every party and resource,
  including acting as any organization **and impersonating any person**.
  Every such action lands in the audit under the operator's name with the hat.
- Operator authority is a delegated relation, held by a few, never by every
  employee. Delegation is done on the deployment itself.
- Tenant-facing audit visibility is a product setting; the operator sees all.
- Humans are always parties; what an organization knows about a human is a
  resource it owns.

## A. Becoming and staying the operator

1. On a fresh deployment I become the first operator by registering an address
   configured ahead of time; nobody can become a second one that way.
   *(isoastra: same, then I delegate.)*
2. I delegate operator authority to another person and take it back; the record
   shows who granted what and when. *(ronitnath: audience of one, but the
   mechanism is the same relation isoastra uses.)*
3. My operator session is short-lived and I can require re-authentication for
   platform actions. A second factor is mandatory for operators once passkeys
   exist.

## B. Knowing the deployment's state

4. One screen tells me: nodes and their raft roles, version per node, whether a
   rollout is mid-way, pending observations, feed growth, key ages.
5. I see every product enabled on this deployment and enable or disable one
   from that screen, immediately, without a redeploy; a disabled product's
   routes vanish and its data stays. *(ronitnath: presence, events…; isoastra:
   company, rinity, services…)*
6. I see signing keys and rotate them with an overlap window, and which relying
   parties are registered and when each last issued a token.

## C. People and identities

7. I find any person by handle, email or id and see everything the system
   knows: identities, factors, sessions, memberships, what they own, consents.
8. I see proposed merges with the evidence behind each, rule on them, and split
   a merge that was wrong with both persons' history intact.
9. I disable a person, an organization or a group: sessions end, links die,
   tokens die, nothing they own disappears. I re-enable and everything is back.
10. I revoke any session, any invitation link, any relying-party consent, and
    the owner can see that I did.
11. I act as any organization and **sign in as any person** (impersonation):
    the audit names me as the actor and them as the hat; the impersonated
    person's own view shows that it happened.

## D. Ownership and rulings

12. I transfer anything to another owner and revoke any relation when a dispute
    or a mistake calls for it.
13. I see the audit for the whole deployment, filterable by actor, hat, object
    and time; every ruling I make is in it. *(isoastra: tenants don't see this
    view unless their product turns it on.)*

## E. Relying parties (OIDC)

14. I register a client (name, redirect URIs, auth method, trusted or not), get
    the secret once, rotate it, delete it; consents under it go with it.
15. I see who has consented to which client and revoke consents in bulk when a
    client is retired.

## F. Data lifecycle

16. I take a consistent backup of the deployment's state and restore it onto a
    fresh formation, proven by the health screen.
17. I export everything an organization owns as a portable unit and import one.
    *(The graduation/portability primitive; may land later, the story is the
    platform's.)*
18. I wipe a disposable-dev deployment and bootstrap it again in one command.

## G. Testing and operating safely

19. On a dev build I sign in as the operator with one click; a release build
    has no such door.
20. I run the platform locally as a three-voter cluster, kill nodes, and the
    health screen tells the truth. I can also bring up an **ephemeral
    single-node instance per worktree** (own ports, own state, own key, dev
    flag) so several people or agents can test in parallel on one machine.
21. When something is wrong I have a way in that doesn't depend on the web
    tier: a CLI against the store, or a node-local operator route.
