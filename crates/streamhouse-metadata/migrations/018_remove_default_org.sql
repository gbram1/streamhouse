-- Remove the default organization record.
-- All production users have real orgs via Clerk; the default org creates confusion
-- and security risk (unauthenticated requests fall through to it).
--
-- NOTE: We cannot ALTER COLUMN to remove DEFAULT in SQLite, but we can delete
-- the seeded row. The application code no longer falls back to this org.

DELETE FROM organizations WHERE id = '00000000-0000-0000-0000-000000000000';
