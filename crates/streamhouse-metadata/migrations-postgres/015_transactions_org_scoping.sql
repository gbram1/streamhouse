-- Add organization_id to transactions for multi-tenant isolation
-- Child tables (transaction_partitions, transaction_markers) inherit org context via FK

ALTER TABLE transactions ADD COLUMN organization_id TEXT;

-- Backfill from producers table via FK
UPDATE transactions SET organization_id = (
    SELECT p.organization_id FROM producers p WHERE p.id = transactions.producer_id
);

-- Default any remaining NULLs to the default org
UPDATE transactions SET organization_id = '00000000-0000-0000-0000-000000000000'
WHERE organization_id IS NULL;

-- Index for org-scoped queries
CREATE INDEX IF NOT EXISTS idx_transactions_org_state ON transactions(organization_id, state);
