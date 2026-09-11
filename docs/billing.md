# Billing integration

Ronitnath.com is the storefront and authorization boundary for the private
`@isoastra/fleet-{ledger,accounting,billing}` packages. The app starts with USD,
manual receipt evidence, one-time offers, and flat monthly or annual memberships.
Ronit and Isoastra sellers map to separate books and are initialized disabled.

## State and accounting boundaries

- Published catalog terms are immutable. Draft publication and its billing version
  commit in one transaction; accepted orders retain that version.
- Orders, invoices, receipt claims, ledger payments, memberships, service intervals,
  fulfillment, adjustments, and entitlement results remain separate records.
- A customer claim never posts money. An operator-confirmed claim atomically records
  the receipt, allocates it, updates the order, and activates eligible service.
- Invoice postings credit deferred revenue. Fulfillment or completed service posts
  recognition. Credits and externally completed refunds retain their own evidence.
- Every financial command has an operation key. Mutable app records use expected
  versions. Entitlements check server time and service boundaries on every read.
- Payment-first access requires full payment. Invoice-first access begins at
  acceptance. Trials begin at acceptance. Late renewal payments retain the invoiced
  interval. Cancellation stops renewal and preserves debt.

## Verification map

| Requirement | Property/model | Application evidence |
|---|---|---|
| Balanced, bounded money effects | Lean conservation/allocation/refund proofs; Quint invariants | Native PostgreSQL invoice/payment suite |
| At-most-once effects | Lean operation-key theorem; Quint retry transitions | Package retry suites and app operation keys |
| No early paid access | Quint access invariant | Commerce unit tests and partial-payment browser journey |
| Immutable accepted terms | Quint publication/revision actions | Catalog publication browser journey and versioned rows |
| Independent seller authority | Quint authorization input | Separate disabled seller mappings and ledger grants |
| Manual evidence boundary | Quint claim/confirm transitions | Customer claim then operator confirmation journey |
| Calendar renewal semantics | Quint time transitions | Month-end/leap-year unit tests and anchored membership rows |
| Cross-customer denial | Authorization property | Browser request returns 404 |
| Atomic app/accounting writes | Transition atomicity | Outer-transaction rollback against PostgreSQL 16 |

The package gate reports proof classes separately: Lean kernel proofs, TLC finite
exhaustion, Apalache bounded checking, generated Quint trace replay, PostgreSQL
adapter tests, and implementation tests. Model domains and bounds are emitted by the
formal check. These checks establish the stated properties within their documented
boundaries; they do not imply completeness for unstated product requirements.

## Operator runbook

1. Run `pnpm db:migrate`. This replays checksummed package and app migrations and
   creates explicit disabled seller mappings.
2. Open `/o/isoastra/billing`, save and publish a catalog, inspect the immutable
   version, then enable the intended seller.
3. Confirm receipt claims only after examining external evidence. Record fulfillment,
   credits, and completed refunds as separate commands.
4. Run `pnpm gate`, `pnpm test:billing:postgres`, and
   `pnpm test:billing:browser` before activation.

Processor APIs, outbound payment attempts, automatic messaging, event capacity,
party-invite reservations, and private-social enforcement are later integrations.
