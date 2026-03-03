-- Add min/max timestamp columns to segments for efficient timestamp-based lookups.
-- These enable the Kafka ListOffsets handler to skip irrelevant segments without
-- downloading them from S3.

ALTER TABLE segments ADD COLUMN min_timestamp BIGINT;
ALTER TABLE segments ADD COLUMN max_timestamp BIGINT;

UPDATE segments SET min_timestamp = created_at, max_timestamp = created_at WHERE min_timestamp IS NULL;

ALTER TABLE segments ALTER COLUMN min_timestamp SET NOT NULL;
ALTER TABLE segments ALTER COLUMN max_timestamp SET NOT NULL;
