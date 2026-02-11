-- Add numeric producer ID mapping for Kafka protocol compatibility.
-- Kafka uses i64 producer IDs; StreamHouse uses UUID strings internally.
ALTER TABLE producers ADD COLUMN numeric_id INTEGER;
CREATE UNIQUE INDEX IF NOT EXISTS idx_producers_numeric_id ON producers(numeric_id);

-- Atomic sequence for allocating numeric producer IDs.
CREATE TABLE IF NOT EXISTS producer_id_sequence (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    next_id INTEGER NOT NULL DEFAULT 1
);
INSERT OR IGNORE INTO producer_id_sequence (id, next_id) VALUES (1, 1);
