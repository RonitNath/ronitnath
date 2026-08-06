---
id: EV-016
date: 2026-08-05
provenance: ground-truth
source: owner, design-gate review of the flow boards
---
"I've also realized that the admin panel is also typically per-event."

The per-invite bespoke ruling (EV-014) extends to the owner-facing side: the admin panel for an event is composed by the creating agent from shared components plus event-specific ones, the same way the invite page is. B24 needed a schedule tracker; July 4th needed segment counts; pickleball needs neither.

Consequence for the design gate's base screen: EV-014's ruling ("the admin overview is the base screen") no longer holds as stated, because the per-event admin overview is now *also* bespoke — it has no canonical layout to lock either. The design language's real home is the set of surfaces that genuinely are platform-level — sign-in, the event index, the cross-event people view, link management — plus the **shared component library** the per-event panels compose from. See DEC-005.

Consequence for the architecture: with both the guest-facing and owner-facing surfaces authored per event, the load-bearing platform assets are the component library and a stable data API for those components to read and write. PKT-04 (agent-api) stops being a convenience for creation and becomes the contract every bespoke panel depends on.
