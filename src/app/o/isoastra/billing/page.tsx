import { randomUUID } from 'node:crypto';
import { readDraft } from '@isoastra/fleet-configured';
import { database } from '@/db/client';
import { requireOperator } from '@/lib/tiers';
import { AddPilotAccountForm, CatalogEditor, ConfirmReceiptButton, CreditForm, FulfillButton, OperatorMembershipCommands, PilotAccountCommand, PilotOfferEditor, RefundForm } from '@/features/billing/components';
import { operatorMemberships, operatorOrders, pilotAccounts, reconciliationQueue, sellers } from '@/features/billing/queries';
import { formatUsd } from '@/features/billing/model';
import { encodeId } from '@/lib/ids';

export const dynamic = 'force-dynamic';
const initial = { offers: [
  { id: 'support', title: 'Support Ronit', description: 'A one-time contribution.', kind: 'one_time', amountAtoms: '2500', interval: null, accessPolicy: 'payment_first', trialDays: 0, paymentDueDays: 7, graceDays: 0, fulfillment: 'immediate', benefits: {}, available: true },
] };

export default async function BillingOperatorPage() {
  await requireOperator();
  const [sellerRows, claims, orders, memberships, pilotRows] = await Promise.all([sellers(), reconciliationQueue(), operatorOrders(), operatorMemberships(), pilotAccounts()]);
  const drafts = await Promise.all(sellerRows.map(async (seller) => ({ seller, draft: await readDraft(database(), 'isoastra', `billing.catalog.${seller.id}`) })));
  return <main className="indoors">
    <h1>Billing</h1><p className="note">Published terms are immutable. Sellers remain unavailable until explicitly enabled.</p>
    <section><h2>Pilot accounts</h2><p className="note">Only enabled accounts can discover offers or make new purchases.</p><AddPilotAccountForm/>{pilotRows.map(({ pilot, person }) => <article className="card" key={pilot.personId}><h3>{person.displayName}</h3><p><span className="mono">{encodeId('person', pilot.personId)}</span> · {pilot.enabled ? 'enabled' : 'disabled'}</p><PilotAccountCommand personId={pilot.personId} version={pilot.version} enabled={pilot.enabled}/></article>)}</section>
    {drafts.map(({ seller, draft }) => <section key={seller.id}><h2>{seller.name}</h2><p className="note">Book <span className="mono">{seller.bookId}</span> · {seller.enabled ? 'enabled' : 'disabled'} · draft {draft.version}</p><h3>Structured one-time pilot offer</h3><PilotOfferEditor seller={seller.id} version={draft.version}/><details><summary>Advanced catalog JSON</summary><CatalogEditor seller={seller.id} version={draft.version} sellerVersion={seller.version} body={draft.body ?? initial} enabled={seller.enabled}/></details></section>)}
    <section><h2>Payment evidence awaiting confirmation</h2>{claims.length ? <div className="scroller"><table className="rows"><thead><tr><th>Customer</th><th>Invoice</th><th>Amount</th><th>Evidence</th><th>Command</th></tr></thead><tbody>{claims.map(({ claim, order, person }) => <tr key={claim.id}><td>{person.displayName}</td><td className="mono">{claim.invoiceId}</td><td>{formatUsd(claim.amountAtoms)}</td><td>{claim.evidence}</td><td><ConfirmReceiptButton claim={claim.id} claimVersion={claim.version} orderVersion={order.version} operationKey={`confirm:${randomUUID()}`}/></td></tr>)}</tbody></table></div> : <p className="empty">No pending claims.</p>}</section>
    <section><h2>Orders and adjustments</h2><div className="scroller"><table className="rows"><thead><tr><th>Customer</th><th>Seller</th><th>Offer</th><th>State</th><th>Balance</th><th>Commands</th></tr></thead><tbody>{orders.map(({ order, person, seller }) => { const outstanding = order.totalAtoms - order.paidAtoms - order.creditedAtoms, refundable = order.paidAtoms - order.refundedAtoms; return <tr key={order.id}><td>{person.displayName}</td><td>{seller.name}</td><td>{order.offerId} v{order.offerVersion}</td><td>{order.state}</td><td>{formatUsd(outstanding)}{order.refundedAtoms ? ` · refunded ${formatUsd(order.refundedAtoms)}` : ''}</td><td>{order.kind === 'one_time' && !order.fulfilledAt && order.state !== 'service_revoked' ? <FulfillButton order={order.id} version={order.version} operationKey={`fulfill:${randomUUID()}`}/> : null}{outstanding > 0n ? <CreditForm order={order.id} version={order.version} maximum={outstanding} operationKey={`credit:${randomUUID()}`}/> : null}{refundable > 0n ? <RefundForm order={order.id} version={order.version} maximum={refundable} operationKey={`refund:${randomUUID()}`}/> : null}</td></tr>; })}</tbody></table></div></section>
    <section><h2>Membership operations</h2>{memberships.map(({ membership, person }) => <article className="card" key={membership.id}><h3>{person.displayName} · {membership.state}</h3><p>{membership.serviceStartsAt.toISOString()} – {membership.serviceEndsAt.toISOString()}</p><OperatorMembershipCommands membership={membership.id} version={membership.version} suspended={!!membership.suspendedAt} operationKey={`renew:${randomUUID()}`}/></article>)}</section>
  </main>;
}
