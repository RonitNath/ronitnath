import { randomUUID } from 'node:crypto';
import { requireSubjectPerson } from '@/lib/tiers';
import { billingForCustomer, publishedOffers } from '@/features/billing/queries';
import { BuyButton, MembershipCommands, PaymentClaimForm } from '@/features/billing/components';
import { formatUsd } from '@/features/billing/model';
import { membershipStatus } from '@/features/billing/actions';

export const dynamic = 'force-dynamic';
export default async function CustomerBillingPage({ params }: { params: Promise<{ user: string }> }) {
  const { user } = await params;
  const context = await requireSubjectPerson(user, 'billing');
  const [catalog, billing] = await Promise.all([publishedOffers(context.subjectPersonId), billingForCustomer(context.subjectPersonId)]);
  const statuses = await Promise.all(billing.memberships.map((row) => membershipStatus(row.id)));
  const subscriptionOffers = catalog.filter((row) => row.offer.kind === 'subscription').map((row) => ({ id: row.offer.id, title: row.offer.title }));
  return <main className="indoors">
    <h1>Billing</h1><p className="note">Invoices, payment evidence, memberships, and receipts for {context.subjectName}.</p>
    {!context.viewingOther ? <section><h2>Available offers</h2><div className="cards">{catalog.map(({ seller, offer }) => <article className="card" key={`${seller.id}:${offer.id}`}><h3>{offer.title}</h3><p>{offer.description}</p><p><strong>{formatUsd(offer.amountAtoms)}</strong></p><BuyButton seller={seller.id} offer={offer.id} version={offer.version} operationKey={`order:${randomUUID()}`}/></article>)}</div>{!catalog.length ? <p className="empty">No pilot offers are available for this account.</p> : null}</section> : null}
    <section><h2>Invoices</h2>{billing.orders.length ? billing.orders.map((order) => { const outstanding = order.totalAtoms - order.paidAtoms - order.creditedAtoms; return <article className="card" key={order.id}><h3>{order.offerId} <span className="note">v{order.offerVersion}</span></h3><p>{order.state} · total {formatUsd(order.totalAtoms)} · paid {formatUsd(order.paidAtoms)} · credited {formatUsd(order.creditedAtoms)} · refunded {formatUsd(order.refundedAtoms)}</p>{!context.viewingOther && outstanding > 0n ? <PaymentClaimForm key={`${order.id}:${order.version}`} invoice={order.invoiceId} version={order.version} outstanding={outstanding} operationKey={`claim:${randomUUID()}`}/> : null}</article>; }) : <p className="empty">No invoices.</p>}</section>
    <section><h2>Memberships</h2>{billing.memberships.map((membership, index) => <article className="card" key={membership.id}><h3>{statuses[index]?.state ?? membership.state}</h3><p>{membership.serviceStartsAt.toLocaleString()} – {membership.serviceEndsAt.toLocaleString()}</p>{membership.cancelAt ? <p>Renewal ends {membership.cancelAt.toLocaleString()}.</p> : null}{!context.viewingOther ? <MembershipCommands membership={membership.id} version={membership.version} offers={subscriptionOffers}/> : null}</article>)}{!billing.memberships.length ? <p className="empty">No membership.</p> : null}</section>
    <section><h2>Payment evidence</h2>{billing.claims.map((claim) => <p key={claim.id}><span className="mono">{claim.evidence}</span> · {formatUsd(claim.amountAtoms)} · {claim.state}{claim.appliedAtoms !== null ? ` · applied ${formatUsd(claim.appliedAtoms)} · unapplied credit ${formatUsd(claim.unappliedAtoms ?? 0n)}` : ''}</p>)}{!billing.claims.length ? <p className="empty">No receipts or pending evidence.</p> : null}</section>
  </main>;
}
