---
id: EV-009
date: 2026-08-06
provenance: ground-truth
source: owner, voice PM interview 2026-08-06 (dictated)
---
"So console was a good attempt, but it essentially attempted to run before it could walk. There were a number of systems which I wasn't confident about, and it was overbuilt, and it presented itself as having a lot of working systems. But when I tried to actually use it, there were many things that were broken, and the UI was essentially a theater to me at that point. It was not something that I could trust."

The post-mortem on `~/dev/console`, and the sharpest statement of what this bet is reacting
against. Three distinct failures: built above its own foundation, built too much, and —
worst — *claimed* function it did not have. "The UI was essentially a theater" is the
indictment: a surface that renders confidently regardless of whether the machinery beneath
it works. Consequence, and it binds the design gate as hard as the build: every surface in
this product must show what was actually observed, never a plausible rendering of it — the
same rule as `feedback_counters_must_name_what_they_witnessed`. A component is not done
because a screen for it exists; it is done when the owner has exercised it and trusts it.
Trust, not feature count, is the unit of progress here. Reads with EV-029 (why the
composition failed) and EV-005 (why increments exist).
