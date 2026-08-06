---
id: EV-034
date: 2026-08-06
provenance: ground-truth
source: owner, requirement 4 (portfolio session 2026-08-06)
---
"the calls ought to get graded by LLM, thus there are per-call rating on particular aspects
of how the voice agent behaved, including doing analysis of the caller's emotions, and how
efficaciously/quickly the call was resolved"

Every call gets an **LLM-generated grade**: per-aspect ratings of the agent's behavior,
caller-emotion analysis, and resolution efficacy/speed. Grades are first-class call-record
data — they feed the review queue (F-4), the client quality analytics (EV-035 companion,
EV-036), and quality triage in fleet ops. Note: audgent's QA machinery exists (EV-011) but
defaults off pre-compliance (D2-6); once real patient data flows, grading LLM calls ride the
compliant model hosting (EV-018).
