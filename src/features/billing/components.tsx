'use client';

import { useActionState, useEffect, useState } from 'react';
import type { FormState } from '@/features/auth/form-state';
import {
  addPilotAccount, cancelRenewal, changeMembership, claimPayment, confirmReceipt, creditOrder,
  fulfillOrder, publishCatalog, recordExternalRefund, renewMembership, requestInvoice,
  saveCatalogDraft, savePilotOfferDraft, setMembershipSuspended, setPilotAccountEnabled, setSellerEnabled,
} from './actions';

type Action = (state: FormState, form: FormData) => Promise<FormState>;
function Command({ action, label, operationKey: suppliedOperationKey, children }: { action: Action; label: string; operationKey?: string; children?: React.ReactNode }) {
  const [state, run, pending] = useActionState(action, {});
  const [operationKey, setOperationKey] = useState(suppliedOperationKey ?? '');
  useEffect(() => { if (!suppliedOperationKey) setOperationKey(crypto.randomUUID()); }, [suppliedOperationKey]);
  return <form action={run} className="stack compact"><input type="hidden" name="operationKey" value={operationKey}/>{children}<button type="submit" disabled={pending || !operationKey}>{pending ? 'Working…' : label}</button>{state.error ? <p className="error">{state.error}</p> : null}{state.notice ? <p className="notice">{state.notice}</p> : null}</form>;
}

export function CatalogEditor({ seller, version, sellerVersion, body, enabled }: { seller: string; version: number; sellerVersion: number; body: unknown; enabled: boolean }) {
  return <div className="stack">
    <Command action={saveCatalogDraft} label="Save draft"><input type="hidden" name="seller" value={seller}/><input type="hidden" name="version" value={version}/><label>Catalog JSON<textarea name="catalog" rows={18} defaultValue={JSON.stringify(body, null, 2)}/></label></Command>
    <div className="actions"><Command action={publishCatalog} label="Publish immutable version"><input type="hidden" name="seller" value={seller}/><input type="hidden" name="version" value={version}/></Command><Command action={setSellerEnabled} label={enabled ? 'Disable seller' : 'Enable seller'}><input type="hidden" name="seller" value={seller}/><input type="hidden" name="version" value={sellerVersion}/><input type="hidden" name="enabled" value={String(!enabled)}/></Command></div>
  </div>;
}

export function PilotOfferEditor({ seller, version }: { seller: string; version: number }) {
  return <Command action={savePilotOfferDraft} label="Save pilot offer draft"><input type="hidden" name="seller" value={seller}/><input type="hidden" name="version" value={version}/><label>Offer key<input name="offerId" defaultValue="purchase" required/></label><label>Title<input name="title" required/></label><label>Description<textarea name="description" rows={3}/></label><label>Price (USD cents)<input name="amountAtoms" inputMode="numeric" required/></label><label>Payment due (days)<input name="paymentDueDays" type="number" min="0" max="3660" defaultValue="7"/></label><label>Fulfillment<select name="fulfillment" defaultValue="operator_confirmed"><option value="operator_confirmed">Operator confirmed</option><option value="immediate">Immediate</option></select></label><label>Availability<select name="available" defaultValue="true"><option value="true">Available when published</option><option value="false">Unavailable</option></select></label></Command>;
}

export function AddPilotAccountForm() { return <Command action={addPilotAccount} label="Enable pilot account"><label>Public person ID<input name="person" placeholder="p_…" required/></label></Command>; }
export function PilotAccountCommand({ personId, version, enabled }: { personId: number; version: number; enabled: boolean }) { return <Command action={setPilotAccountEnabled} label={enabled ? 'Disable new purchases' : 'Enable new purchases'}><input type="hidden" name="person" value={personId}/><input type="hidden" name="version" value={version}/><input type="hidden" name="enabled" value={String(!enabled)}/></Command>; }

export function BuyButton({ seller, offer, version, operationKey }: { seller: string; offer: string; version: number; operationKey: string }) {
  return <Command action={requestInvoice} label="Request invoice" operationKey={operationKey}><input type="hidden" name="seller" value={seller}/><input type="hidden" name="offer" value={offer}/><input type="hidden" name="version" value={version}/></Command>;
}

export function PaymentClaimForm({ invoice, version, outstanding, operationKey }: { invoice: string; version: number; outstanding: bigint; operationKey: string }) {
  return <Command action={claimPayment} label="Submit payment evidence" operationKey={operationKey}><input type="hidden" name="invoice" value={invoice}/><input type="hidden" name="version" value={version}/><label>Amount (cents)<input name="amountAtoms" inputMode="numeric" defaultValue={outstanding.toString()}/></label><label>Evidence or reference<input name="evidence" required maxLength={2000}/></label></Command>;
}

export function ConfirmReceiptButton({ claim, claimVersion, orderVersion, operationKey }: { claim: string; claimVersion: number; orderVersion: number; operationKey: string }) {
  return <Command action={confirmReceipt} label="Confirm receipt" operationKey={operationKey}><input type="hidden" name="claim" value={claim}/><input type="hidden" name="claimVersion" value={claimVersion}/><input type="hidden" name="version" value={orderVersion}/><label>Source<input name="externalNamespace" defaultValue="manual" required/></label><label>Receipt ID<input name="externalId" required/></label></Command>;
}

export function FulfillButton({ order, version, operationKey }: { order: string; version: number; operationKey: string }) {
  return <Command action={fulfillOrder} label="Record fulfillment" operationKey={operationKey}><input type="hidden" name="order" value={order}/><input type="hidden" name="version" value={version}/></Command>;
}

export function CreditForm({ order, version, maximum, operationKey }: { order: string; version: number; maximum: bigint; operationKey: string }) {
  return <Command action={creditOrder} label="Issue credit" operationKey={operationKey}><input type="hidden" name="order" value={order}/><input type="hidden" name="version" value={version}/><label>Amount (cents)<input name="amountAtoms" inputMode="numeric" max={maximum.toString()}/></label><label>Reason<input name="evidence" maxLength={2000}/></label><label>Service effect<select name="serviceEffect"><option value="preserve_service">Preserve service</option><option value="revoke_unfulfilled_service">Revoke unfulfilled service</option></select></label></Command>;
}

export function RefundForm({ order, version, maximum, operationKey }: { order: string; version: number; maximum: bigint; operationKey: string }) {
  return <Command action={recordExternalRefund} label="Record external refund" operationKey={operationKey}><input type="hidden" name="order" value={order}/><input type="hidden" name="version" value={version}/><label>Amount (cents)<input name="amountAtoms" inputMode="numeric" max={maximum.toString()}/></label><label>Reason<input name="evidence" required maxLength={2000}/></label><label>Source<input name="externalNamespace" defaultValue="manual" required/></label><label>External refund ID<input name="externalId" required/></label><label>Service effect<select name="serviceEffect"><option value="preserve_service">Preserve service</option><option value="revoke_unfulfilled_service">Revoke unfulfilled service</option></select></label></Command>;
}

export function OperatorMembershipCommands({ membership, version, suspended, operationKey }: { membership: string; version: number; suspended: boolean; operationKey: string }) {
  return <div className="actions"><Command action={renewMembership} label="Advance cycle" operationKey={operationKey}><input type="hidden" name="membership" value={membership}/><input type="hidden" name="version" value={version}/></Command><Command action={setMembershipSuspended} label={suspended ? 'Resume' : 'Suspend'}><input type="hidden" name="membership" value={membership}/><input type="hidden" name="version" value={version}/><input type="hidden" name="suspend" value={String(!suspended)}/></Command></div>;
}

export function MembershipCommands({ membership, version, offers }: { membership: string; version: number; offers: Array<{ id: string; title: string }> }) {
  return <div className="actions"><Command action={changeMembership} label="Schedule tier"><input type="hidden" name="membership" value={membership}/><input type="hidden" name="version" value={version}/><select name="offer">{offers.map((o) => <option key={o.id} value={o.id}>{o.title}</option>)}</select></Command><Command action={cancelRenewal} label="Cancel renewal"><input type="hidden" name="membership" value={membership}/><input type="hidden" name="version" value={version}/></Command></div>;
}
