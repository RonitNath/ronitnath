CREATE TABLE audience_circle_grants (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    policy_id INTEGER NOT NULL REFERENCES audience_policies(id) ON DELETE CASCADE,
    circle_id INTEGER NOT NULL REFERENCES circles(id) ON DELETE CASCADE,
    level TEXT NOT NULL CHECK (level IN ('hidden', 'busy', 'summary', 'full')),
    UNIQUE(policy_id, circle_id)
);
