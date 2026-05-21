-- Rename clerk_id to external_id (vendor-neutral naming)
ALTER TABLE organizations RENAME COLUMN clerk_id TO external_id;

-- Update index name
DROP INDEX IF EXISTS idx_organizations_clerk_id;
CREATE UNIQUE INDEX idx_organizations_external_id ON organizations (external_id) WHERE external_id IS NOT NULL;
