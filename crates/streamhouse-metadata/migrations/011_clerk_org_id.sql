-- Add Clerk organization ID for mapping Clerk orgs to StreamHouse orgs
ALTER TABLE organizations ADD COLUMN clerk_id TEXT UNIQUE;
