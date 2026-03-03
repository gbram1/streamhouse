-- Add min/max timestamp columns to segments for efficient timestamp-based lookups.
-- These enable the Kafka ListOffsets handler to skip irrelevant segments without
-- downloading them from S3.

ALTER TABLE segments ADD COLUMN min_timestamp INTEGER NOT NULL DEFAULT 0;
ALTER TABLE segments ADD COLUMN max_timestamp INTEGER NOT NULL DEFAULT 0;
