---
id: EV-013
date: 2026-08-05
provenance: inference
source: agent catalog of real artifacts — repos RonitNath/{socials,rn-events,events} + events/src/seed.rs recovered content; verifiable
---
The three past events were **three separate codebases**, each rebuilt for one event. What each actually needed:

**Housewarming (Nov 2025, `socials`)** — Rust+sqlx, islands frontend. One event page, RSVP with party size ("Joe with Jaysa, Sophie, one_more"), login island, admin FAQs, photo-guided entry instructions (BART exit → building → front desk → floor 22). No schedule. Style: Tailwind replaced by hand-rolled "neoclassical" styles mid-build.

**B24 (Mar 2026, `rn-events`)** — Rust+Astro. Already a mini-CMS: admin EventForm/EventList/ScheduleEditor/GuestBulkAdd/GuestTable/**ImageManager**/**ThemeEditor**; public InvitePage, PlusOnes, RsvpControls, background components (Gradient, **Starfield** — theme "Starlight"), image-processor service, custom font (AGENCYB). Simple public schedule (5 time-label items). Migrated socials data in.

**July 4th (Jul 2026, `events`)** — stage_2 fork. Capability links with public/private tiers; personalized `name-xxxx` invites ("Hi Name" animated wordmark, animated GIF og-preview); notice/quick-plan invite banners; guest **status manager** (not RSVP) + Telegram host pings; **segmented schedule** (board games → dinner → fireworks → rooftop → sleepover), per-segment RSVP + capacity + paid/attended bookkeeping (Zelle dinner); longitudinal people table accumulating attendance across events; admin overview dashboard (headcounts, link activity, QR); night-sky design system + **canvas fireworks particle system**; published/archived landing. Deliberately REMOVED B24's web-form editing — heavy edits went to CLI/agents.

**Variance axes that recur**: (1) per-event visual identity — neoclassical → starlight/starfield → night-sky/fireworks, each a real design effort, not a color swap; (2) schedule complexity — none → flat list → tiered segments with capacity and payments; (3) RSVP semantics — party-size RSVP → plus-ones → ongoing status manager; (4) invite/access model — login → slug pages → tiered capability links with personalization; (5) media — none → managed image uploads → static recovered photos. Entry-instructions-with-photos and guest-list continuity were needed by all three.

**Irony worth keeping**: B24 had the CMS the owner now asks for; July 4th deliberately deleted it in favor of agent/CLI editing. The new want (EV-012: agent creates, human tweaks in UI) is a synthesis of both, not a regression of either.
