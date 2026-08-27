# Agents — user stories

Owner ruling 2026-08-27: **AI coding agents are first-class actors.** "I just
open my terminal and tell it to get to work — no auth dance. You already live
on my cluster and have access to everything. It is technically me through an
agent, but it should appear separately from my identity."

## Shape

- An agent is a **party of kind `agent`** — its own name, its own audit
  identity — **delegated by a person** (`person:ronit #delegates @agent:X`).
  It never has its own sessions, factors or memberships.
- **Authority is the delegator's, exactly.** `check()` for an agent resolves
  through the delegating person (and whatever that person is acting as);
  nothing is granted to the agent itself, so revoking the delegation removes
  everything at once. An agent cannot enrol agents, change the delegator's
  factors, or grant operator authority.
- **Attribution is the agent's.** Every audit row an agent writes names the
  agent as actor and the delegating person as the hat. The person's own view
  shows "what my agents did"; a guest who looks at an event's history sees the
  host, not the machinery — the split is internal, not a product surface.
- **Enrolment is provisioning, not sign-in.** A machine that hosts agents gets
  a credential once — written by the operator (the fleet config on cluster
  nodes; `rn agent enrol` on a laptop, run by the person, once) into a
  file only that user can read. From then on the agent's tooling reads it;
  there is no browser, no token exchange, no expiry the human has to notice.
  Revocation is a command; rotation is a command.
- **The surface is a CLI first** (`rn …`: commands, queries, and a few
  composed verbs like `rn event create`), reading the credential from the
  machine and speaking to the deployment's API; an MCP wrapper over the same
  CLI later. The API accepts the agent credential on `/api/cmd|q` with its own
  extractor — the *only* non-cookie principal `/api` admits; invitation links
  still never enter it.

## Stories

1. **Enrol once.** On my Mac I run one command as myself; on the cluster the
   fleet config does it. From then on any agent I run on that machine acts as
   "claude-code on <host>", delegated by me. I see every enrolled agent on my
   own page with its host, its last action, and a revoke button.
2. **Tell it to work.** In a terminal I say "make the Sunday event and invite
   the SF group"; the agent runs `rn` commands; the event exists, owned by me,
   and the audit reads *claude-code (for Ronit): create-event*.
3. **Separate but mine.** My identity page does not list the agent's actions
   as my sessions; the agent's page lists them as its own, with me as the
   reason it was allowed.
4. **Bounded exactly by me.** If I can't do something, neither can the agent;
   the moment I lose a role, so does it. Disabling me disables every agent I
   delegate.
5. **Operators see agents as parties**: find, disable, revoke, audit them like
   anyone else; an operator's agent has operator authority, and the audit
   says so.
6. **Sensitive things stay human**: factor changes, operator grants, enrolling
   agents, impersonation — refused from an agent, with a refusal the agent can
   read and report back to me.

## What this asks of the platform

- `party.kind` gains `agent`; a `delegates` relation; an `agent` resource with
  host, credential hash, enrolled_at, revoked_at, last_seen.
- `Principal::Agent { agent, delegator }`; `refs::actor` = agent, hat =
  delegator; `authority::allows` resolves through the delegator; a named set of
  commands refused to agents (the human-only set).
- Enrolment (`enrol-agent`, `revoke-agent`, `rotate-agent-credential`)
  commands; a fleet-side provisioning path for cluster nodes.
- Server extractor for the agent credential on `/api/cmd|q`; matrix rows for
  it across every route (it must be refused everywhere else).
- `rn` CLI crate: thin over the API, credential from the machine, composed
  verbs per product; the agent-facing docs are its `--help`.
- Person's page: agents delegated by me; platform page: agents as parties.
