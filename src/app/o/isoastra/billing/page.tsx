import { randomUUID } from 'node:crypto';
import { readDraft } from '@isoastra/fleet-configured';
import { database } from '@/db/client';
import { requireOperator } from '@/lib/tiers';
import { CatalogEditor, ConfirmReceiptButton, CreditForm, FulfillButton, OperatorMembershipCommands, RefundForm } from '@/features/billing/components';
import { operatorMemberships, operatorOrders, reconciliationQueue, sellers } from '@/features/billing/queries';
import { formatUsd } from '@/features/billing/model';

export const dynamic = 'force-dynamic';
const initial = { offers: [
  { id: 'support', title: 'Support Ronit', description: 'A one-time contribution.', kind: 'one_time', amountAtoms: '2500', interval: null, accessPolicy: 'payment_first', trialDays: 0, paymentDueDays: 7, graceDays: 0, fulfillment: 'immediate', benefits: {}, available: true },
  { id: 'member', title: 'Member', description: 'Monthly membership.', kind: 'subscription', amountAtoms: '1000', interval: 'month', accessPolicy: 'payment_first', trialDays: 0, paymentDueDays: 7, graceDays: 0, fulfillment: 'operator_confirmed', benefits: { 'social.private': true }, available: true },
] };

export default async function BillingOperatorPage() {
  await requireOperator();
  const [sellerRows, claims, orders, memberships] = await Promise.all([sellers(), reconciliationQueue(), operatorOrders(), operatorMemberships()]);
  const drafts = await Promise.all(sellerRows.map(async (seller) => ({ seller, draft: await readDraft(database(), 'isoastra', `billing.catalog.${seller.id}`) })));
  return <main className="indoors">
    <h1>Billing</h1><p className="note">Published terms are immutable. Sellers remain unavailable until explicitly enabled.</p>
    {drafts.map(({ seller, draft }) => <section key={seller.id}><h2>{seller.name}</h2><p className="note">Book <span className="mono">{seller.bookId}</span> · {seller.enabled ? 'enabled' : 'disabled'} · draft {draft.version}</p><CatalogEditor seller={seller.id} version={draft.version} body={draft.body ?? initial} enabled={seller.enabled}/></section>)}
    <section><h2>Payment evidence awaiting confirmation</h2>{claims.length ? <div className="scroller"><table className="rows"><thead><tr><th>Customer</th><th>Invoice</th><th>Amount</th><th>Evidence</th><th>Command</th></tr></thead><tbody>{claims.map(({ claim, person }) => <tr key={claim.id}><td>{person.displayName}</td><td className="mono">{claim.invoiceId}</td><td>{formatUsd(claim.amountAtoms)}</td><td>{claim.evidence}</td><td><ConfirmReceiptButton claim={claim.id} operationKey={`confirm:${randomUUID()}`}/></td></tr>)}</tbody></table></div> : <p className="empty">No pending claims.</p>}</section>
    <section><h2>Orders and adjustments</h2><div className="scroller"><table className="rows"><thead><tr><th>Customer</th><th>Seller</th><th>Offer</th><th>State</th><th>Balance</th><th>Commands</th></tr></thead><tbody>{orders.map(({ order, person, seller }) => <tr key={order.id}><td>{person.displayName}</td><td>{seller.name}</td><td>{order.offerId} v{order.offerVersion}</td><td>{order.state}</td><td>{formatUsd(order.totalAtoms - order.paidAtoms)}</td><td>{order.kind === 'one_time' && !order.fulfilledAt ? <FulfillButton order={order.id} version={order.version} operationKey={`fulfill:${randomUUID()}`}/> : null}<CreditForm order={order.id} maximum={order.totalAtoms} operationKey={`credit:${randomUUID()}`}/>{order.paidAtoms > 0n ? <RefundForm order={order.id} maximum={order.paidAtoms} operationKey={`refund:${randomUUID()}`}/> : null}</td></tr>)}</tbody></table></div></section>
    <section><h2>Membership operations</h2>{memberships.map(({ membership, person }) => <article className="card" key={membership.id}><h3>{person.displayName} · {membership.state}</h3><p>{membership.serviceStartsAt.toISOString()} – {membership.serviceEndsAt.toISOString()}</p><OperatorMembershipCommands membership={membership.id} version={membership.version} suspended={!!membership.suspendedAt} operationKey={`renew:${randomUUID()}`}/></article>)}</section>
  </main>;
}
