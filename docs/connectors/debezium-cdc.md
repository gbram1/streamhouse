# Debezium CDC with StreamHouse

[Debezium](https://debezium.io/) is an open-source CDC (Change Data Capture) platform that captures row-level changes from databases and streams them as events. Since StreamHouse speaks the Kafka protocol, Debezium works out of the box.

## PostgreSQL CDC

### Prerequisites

1. PostgreSQL with logical replication enabled:
   ```sql
   -- postgresql.conf
   wal_level = logical
   max_replication_slots = 4
   max_wal_senders = 4
   ```

2. A user with replication permissions:
   ```sql
   CREATE ROLE debezium WITH LOGIN REPLICATION PASSWORD 'dbz';
   GRANT SELECT ON ALL TABLES IN SCHEMA public TO debezium;
   ```

### Connector Configuration

Create `postgres-cdc.properties`:

```properties
name=postgres-cdc
connector.class=io.debezium.connector.postgresql.PostgresConnector
tasks.max=1

# Database connection
database.hostname=localhost
database.port=5432
database.user=debezium
database.password=dbz
database.dbname=myapp

# StreamHouse topic prefix
topic.prefix=cdc

# Tables to capture
table.include.list=public.users,public.orders,public.products

# Snapshot mode
snapshot.mode=initial

# Plugin (use pgoutput for PG 10+)
plugin.name=pgoutput
slot.name=debezium_slot
publication.name=dbz_publication
```

### Run

```bash
connect-standalone.sh connect-standalone.properties postgres-cdc.properties
```

This creates topics in StreamHouse:
- `cdc.public.users` — changes to the `users` table
- `cdc.public.orders` — changes to the `orders` table
- `cdc.public.products` — changes to the `products` table

### Event Format

Each CDC event contains:

```json
{
  "before": null,
  "after": {
    "id": 1001,
    "name": "John Doe",
    "email": "john@example.com",
    "created_at": "2026-01-15T10:30:00Z"
  },
  "source": {
    "version": "2.5.0",
    "connector": "postgresql",
    "name": "cdc",
    "ts_ms": 1705312200000,
    "db": "myapp",
    "schema": "public",
    "table": "users"
  },
  "op": "c",
  "ts_ms": 1705312200123
}
```

Operations: `c` (create), `u` (update), `d` (delete), `r` (read/snapshot)

## MySQL CDC

### Prerequisites

MySQL with binary logging enabled:
```ini
# my.cnf
server-id=1
log_bin=mysql-bin
binlog_format=ROW
binlog_row_image=FULL
```

### Connector Configuration

```properties
name=mysql-cdc
connector.class=io.debezium.connector.mysql.MySqlConnector
tasks.max=1

database.hostname=localhost
database.port=3306
database.user=debezium
database.password=dbz
database.server.id=184054

topic.prefix=cdc
database.include.list=myapp
table.include.list=myapp.users,myapp.orders

schema.history.internal.kafka.bootstrap.servers=localhost:9092
schema.history.internal.kafka.topic=schema-changes.myapp
```

## MongoDB CDC

### Connector Configuration

```properties
name=mongo-cdc
connector.class=io.debezium.connector.mongodb.MongoDbConnector
tasks.max=1

mongodb.connection.string=mongodb://localhost:27017
mongodb.name=myapp

topic.prefix=cdc
collection.include.list=myapp.users,myapp.orders

# Capture mode
capture.mode=change_streams_update_full
```

## Querying CDC Data with SQL

Once CDC data flows into StreamHouse, query it with SQL:

```sql
-- Recent changes to the users table
SELECT
    key,
    json_extract(value, '$.op') as operation,
    json_extract(value, '$.after.name') as name,
    json_extract(value, '$.after.email') as email,
    timestamp
FROM "cdc.public.users"
ORDER BY offset DESC
LIMIT 20;

-- Count changes per table in the last hour
SELECT
    COUNT(*) as change_count
FROM "cdc.public.orders"
WHERE timestamp >= NOW() - INTERVAL '1' HOUR;

-- Windowed aggregation of order changes
SELECT
    window_start, window_end,
    COUNT(*) as changes,
    SUM(CASE WHEN json_extract(value, '$.op') = 'c' THEN 1 ELSE 0 END) as inserts,
    SUM(CASE WHEN json_extract(value, '$.op') = 'u' THEN 1 ELSE 0 END) as updates,
    SUM(CASE WHEN json_extract(value, '$.op') = 'd' THEN 1 ELSE 0 END) as deletes
FROM "cdc.public.orders"
GROUP BY TUMBLE(timestamp, INTERVAL '5' MINUTE);
```

## Tips

- **Initial snapshot**: Set `snapshot.mode=initial` to capture existing data before streaming changes
- **Schema evolution**: Debezium handles schema changes automatically; StreamHouse's schema registry can track them
- **Monitoring**: Check consumer group lag for the Debezium connector group to ensure it's keeping up
- **Retention**: Set appropriate retention on CDC topics (e.g., 7 days) to avoid unbounded growth
