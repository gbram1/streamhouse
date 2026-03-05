-- Add Clerk organization ID for mapping Clerk orgs to StreamHouse orgs
ALTER TABLE organizations ADD COLUMN clerk_id VARCHAR(255) UNIQUE;
CREATE INDEX idx_organizations_clerk_id ON organizations (clerk_id) WHERE clerk_id IS NOT NULL;
