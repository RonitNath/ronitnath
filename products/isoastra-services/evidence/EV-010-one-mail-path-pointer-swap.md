---
id: EV-010
date: 2026-08-06
provenance: ground-truth
source: owner, mail-fabric session (claude session 6e9d6a54, 2026-08-06)
---
"No wait, I wnat however the product -> mailer -> SES pathway works to be identitcal for product -> mailer -> stalwart or product -> stalwart. Testing the email pathway and being bit-identical is not as important as being able to exercise the email workflows. […] Building multiple backends doesn't make sense to me. There should just be one path, and we change what that path points at for non-production instances"

The mailing domain's test-posture ruling: one delivery path, endpoint injectable by config, so
non-production instances point at the mail-test fabric (Stalwart) with zero code difference. The
current mailer satisfies this by accident (the SES SDK honors `AWS_ENDPOINT_URL_SESV2`); the
successor must satisfy it on purpose. Agents must be able to exercise full product email workflows
(signup → verification mail → read via JMAP → verify → password reset → support back-and-forth)
against test instances.
