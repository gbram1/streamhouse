-- Rename clerk_id to external_id (vendor-neutral naming)
ALTER TABLE organizations RENAME COLUMN clerk_id TO external_id;
