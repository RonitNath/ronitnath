---
id: DEC-001
date: 2026-08-06
source: owner rulings resolving BRIEF-001 OQ-1, OQ-2, OQ-3, OQ-9 (EV-031, EV-032, EV-033, EV-039)
---
Chose **a standalone service with a local SQLite database, audio on the filesystem kept
forever, behind Kanidm** over **a slice of the universe-ronitnath monolith on the shared
PostgreSQL seam**, because voice.ronitnath.com is a single-node, single-user, testing-mode
service (EV-006, EV-013) whose value depends on being trivially resettable and on owning a
permanent measurement corpus — neither of which a shared production data plane gives it.

| Rule | The build must now |
| --- | --- |
| Own repo, own workspace | Not a crate inside universe-ronitnath; no dependency on it. Same stack (EV-008), separate lineage. |
| SQLite, plain SQL | Local file; no ORM magic, keeping the eventual Postgres translation cheap at graduation. No database server prerequisite for rung 1. |
| Audio on the filesystem | Clips stored as files, referenced by row. The database holds the timeline, never the bytes. |
| Append-only, no deletion | No retention policy, no cleanup path, no lifecycle states. Disk growth is monitored, not managed. |
| The audio directory is the irreplaceable state | Backups exist for it. SQLite is rebuildable from it in principle; the corpus is not rebuildable from anything. |
| Kanidm from rung 1 | Real auth on day one, one human principal. `procedures/security.md` applies; internal-only never means unauthenticated. |
| Configuration, not assumption | Hostname, realm, and storage root are config — graduation (EV-007) must be a relocation, not a second rewrite. |

**What was given up knowingly**: the monolith path would have inherited universe-ronitnath's
identity model, deploy pipeline (push-to-`deploy` CD across sfo/nyc/nexus) and Postgres HA
for free, and would have made graduation a matter of moving a module rather than a service.
Standalone pays that setup cost again and will pay a SQLite→Postgres translation later.

**What would change this**: a second consumer appearing before graduation (EV-037 says there
is none), a need for HA or multi-node writes, or a corpus that outgrows single-node disk. Any
of those reopens the seam choice — none of them reopens the standalone/monolith choice, which
EV-031 settles independently.
