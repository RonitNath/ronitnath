ALTER TABLE accounts ADD COLUMN purpose TEXT NOT NULL DEFAULT 'primary'
    CHECK (purpose IN ('primary', 'guest'));
