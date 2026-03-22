-- Remove the default organization record.
-- All production users have real orgs via Clerk; the default org creates confusion
-- and security risk (unauthenticated requests fall through to it).

DELETE FROM organizations WHERE id = '00000000-0000-0000-0000-000000000000';

-- Remove DEFAULT clauses from organization_id columns so new rows must provide one.
ALTER TABLE topics ALTER COLUMN organization_id DROP DEFAULT;
ALTER TABLE partitions ALTER COLUMN organization_id DROP DEFAULT;
ALTER TABLE segments ALTER COLUMN organization_id DROP DEFAULT;
ALTER TABLE consumer_offsets ALTER COLUMN organization_id DROP DEFAULT;
ALTER TABLE partition_leases ALTER COLUMN organization_id DROP DEFAULT;
ALTER TABLE consumer_groups ALTER COLUMN organization_id DROP DEFAULT;
