---
id: EV-014
date: 2026-08-06
provenance: ground-truth
source: owner, PM interview (voice)
---
"the mailer is not a highly available service right now. Right now, DentConnex is using the mailer in order to do mailing, but that is the single point of failure. If that goes down, then none of the mails are going out right now. At the same time, I like having the mailer service centralized instead of sending my SES Amazon credentials to every single service which needs to be able to do emailing because, honestly, a lot of services need to be able to do emailing. […] Technically, I could just take the current mail service and upgrade it, but I've also advanced a lot in terms of the standards I want for the front end for services. And, also, there's a totally new design process for building projects. So I want to take time to also reconceptualize what the front end should look like and rebuild it based on the new standards for how you ought to build things."

Three things settled. (1) The mailer's defect is availability: it is a live SPOF for DentConnex's
mail today. (2) Centralized mail is *kept* deliberately — the value is credential custody (SES keys
live in one place, not in every service that sends), and that value generalizes to the hub thesis
(EV-012). (3) The rebuild-vs-upgrade question is answered as **rebuild**, and the reason is the
front end: the owner's standards for service UIs and the design process (portfolio design gate,
interface minimalism) have both moved past what an in-place upgrade would carry.
