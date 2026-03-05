-- Add Clerk organization ID for mapping Clerk orgs to StreamHouse orgs
ALTER TABLE organizations ADD COLUMN clerk_id TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_organizations_clerk_id ON organizations(clerk_id);
