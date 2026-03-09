-- API key-level rate limits (NULL = use org default)
ALTER TABLE api_keys ADD COLUMN max_requests_per_sec INTEGER;
ALTER TABLE api_keys ADD COLUMN max_produce_bytes_per_sec INTEGER;
ALTER TABLE api_keys ADD COLUMN max_consume_bytes_per_sec INTEGER;
