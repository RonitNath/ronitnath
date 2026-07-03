-- audit_log = one row per attributable mutation, plus pre-auth security
-- events (failed logins). identity_id/account_id are nullable so an event
-- before anyone's identified (e.g. login.failed on an unknown email) can
-- still be logged. request_id joins a row back to the tracing span/logs.
CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY NOT NULL,
    at TEXT NOT NULL DEFAULT (datetime('now')),
    identity_id INTEGER REFERENCES identities(id),
    account_id INTEGER REFERENCES accounts(id),
    request_id TEXT,
    action TEXT NOT NULL,
    entity TEXT NOT NULL,
    entity_id TEXT,
    detail TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_audit_log_account ON audit_log(account_id, at);
