CREATE TABLE audience_person_overrides (
    id INTEGER PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id),
    policy_id INTEGER NOT NULL REFERENCES audience_policies(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    override_kind TEXT NOT NULL CHECK (override_kind IN ('include', 'exclude')),
    level TEXT CHECK (level IN ('hidden', 'busy', 'summary', 'full')),
    UNIQUE(policy_id, person_id),
    CHECK ((override_kind = 'exclude' AND level IS NULL) OR
           (override_kind = 'include' AND level IS NOT NULL))
);

-- A legacy person-bound private link was a Full invitation.
INSERT INTO audience_person_overrides
    (account_id, policy_id, person_id, override_kind, level)
SELECT l.account_id, p.id, l.person_id, 'include', 'full'
FROM event_links l
JOIN audience_policies p
  ON p.account_id = l.account_id AND p.subject_type = 'event' AND p.subject_id = l.event_id
WHERE l.person_id IS NOT NULL AND l.tier = 'private'
GROUP BY l.account_id, p.id, l.person_id;
