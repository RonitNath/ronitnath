'use client';

import { useActionState } from 'react';
import type { FormState } from '@/features/auth/form-state';
import {
  cancelRenewal, changeMembership, claimPayment, confirmReceipt, creditOrder,
  fulfillOrder, publishCatalog, recordExternalRefund, renewMembership, requestInvoice,
  saveCatalogDraft, setMembershipSuspended, setSellerEnabled,
} from './actions';

type Action = (state: FormState, form: FormData) => Promise<FormState>;
function Command({ action, label, children }: { action: Action; label: string; children?: React.ReactNode }) {
  const [state, run, pending] = useActionState(action, {});
  return <form action={run} className="stack compact">{children}<button type="submit" disabled={pending}>{pending ? 'Working…' : label}</button>{state.error ? <p className="error">{state.error}</p> : null}{state.notice ? <p className="notice">{state.notice}</p> : null}</form>;
}
function OperationKey({ value }: { value: string }) { return <input type="hidden" name="operationKey" value={value} />; }

export function CatalogEditor({ seller, version, body, enabled }: { seller: string; version: number; body: unknown; enabled: boolean }) {
  return <div className="stack">
    <Command action={saveCatalogDraft} label="Save draft"><input type="hidden" name="seller" value={seller}/><input type="hidden" name="version" value={version}/><label>Catalog JSON<textarea name="catalog" rows={18} defaultValue={JSON.stringify(body, null, 2)}/></label></Command>
    <div className="actions"><Command action={publishCatalog} label="Publish immutable version"><input type="hidden" name="seller" value={seller}/><input type="hidden" name="version" value={version}/></Command><Command action={setSellerEnabled} label={enabled ? 'Disable seller' : 'Enable seller'}><input type="hidden" name="seller" value={seller}/><input type="hidden" name="enabled" value={String(!enabled)}/></Command></div>
  </div>;
}

export function BuyButton({ seller, offer, operationKey }: { seller: string; offer: string; operationKey: string }) {
  return <Command action={requestInvoice} label="Request invoice"><OperationKey value={operationKey}/><input type="hidden" name="seller" value={seller}/><input type="hidden" name="offer" value={offer}/></Command>;
}

export function PaymentClaimForm({ invoice, outstanding, operationKey }: { invoice: string; outstanding: bigint; operationKey: string }) {
  return <Command action={claimPayment} label="Submit payment evidence"><OperationKey value={operationKey}/><input type="hidden" name="invoice" value={invoice}/><label>Amount (cents)<input name="amountAtoms" inputMode="numeric" defaultValue={outstanding.toString()}/></label><label>Evidence or reference<input name="evidence" required maxLength={2000}/></label></Command>;
}

export function ConfirmReceiptButton({ claim, operationKey }: { claim: string; operationKey: string }) {
  return <Command action={confirmReceipt} label="Confirm receipt"><OperationKey value={operationKey}/><input type="hidden" name="claim" value={claim}/></Command>;
}

export function FulfillButton({ order, version, operationKey }: { order: string; version: number; operationKey: string }) {
  return <Command action={fulfillOrder} label="Record fulfillment"><OperationKey value={operationKey}/><input type="hidden" name="order" value={order}/><input type="hidden" name="version" value={version}/></Command>;
}

export function CreditForm({ order, maximum, operationKey }: { order: string; maximum: bigint; operationKey: string }) {
  return <Command action={creditOrder} label="Issue credit"><OperationKey value={operationKey}/><input type="hidden" name="order" value={order}/><label>Amount (cents)<input name="amountAtoms" inputMode="numeric" max={maximum.toString()}/></label><label>Reason<input name="evidence" maxLength={2000}/></label></Command>;
}

export function RefundForm({ order, maximum, operationKey }: { order: string; maximum: bigint; operationKey: string }) {
  return <Command action={recordExternalRefund} label="Record external refund"><OperationKey value={operationKey}/><input type="hidden" name="order" value={order}/><label>Amount (cents)<input name="amountAtoms" inputMode="numeric" max={maximum.toString()}/></label><label>External reference<input name="evidence" required maxLength={2000}/></label></Command>;
}

export function OperatorMembershipCommands({ membership, version, suspended, operationKey }: { membership: string; version: number; suspended: boolean; operationKey: string }) {
  return <div className="actions"><Command action={renewMembership} label="Advance cycle"><OperationKey value={operationKey}/><input type="hidden" name="membership" value={membership}/><input type="hidden" name="version" value={version}/></Command><Command action={setMembershipSuspended} label={suspended ? 'Resume' : 'Suspend'}><input type="hidden" name="membership" value={membership}/><input type="hidden" name="version" value={version}/><input type="hidden" name="suspend" value={String(!suspended)}/></Command></div>;
}

export function MembershipCommands({ membership, version, offers }: { membership: string; version: number; offers: Array<{ id: string; title: string }> }) {
  return <div className="actions"><Command action={changeMembership} label="Schedule tier"><input type="hidden" name="membership" value={membership}/><input type="hidden" name="version" value={version}/><select name="offer">{offers.map((o) => <option key={o.id} value={o.id}>{o.title}</option>)}</select></Command><Command action={cancelRenewal} label="Cancel renewal"><input type="hidden" name="membership" value={membership}/><input type="hidden" name="version" value={version}/></Command></div>;
}
