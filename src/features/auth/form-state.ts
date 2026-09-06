/* What a form gets back from its action. One shape for all of them: an error
 * to show inside the form, or a notice to show in its place. Never both. */

export interface FormState {
  error?: string;
  notice?: string;
  /* Which form on /auth the answer belongs to, so the other one stays quiet. */
  form?: 'sign-in' | 'register' | 'reset';
}

/* The uniform decline. Design brief: never a field-level hint, because a hint
 * is a lookup service for whoever is asking. */
export const AUTH_FAILED = 'Authentication failed.';
export const CHECK_INBOX = 'Check your inbox. If the address can be used, a link is on its way.';
export const RESET_SENT = 'Check your inbox. If the address has an account, a reset link is on its way.';
