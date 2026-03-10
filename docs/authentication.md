# Authentication

StreamHouse supports API key authentication across all three protocols. One key works everywhere — REST, Kafka, and gRPC.

---

## Enabling Auth

```bash
export STREAMHOUSE_AUTH_ENABLED=true
./target/release/unified-server
```

When enabled, all requests must include a valid API key. Without auth enabled, all requests use the default organization.

---

## API Keys

Keys use the format `sk_live_<random>` and are tied to an organization.

### Create a Key

```bash
curl -X POST http://localhost:8080/api/v1/organizations/{org_id}/api-keys \
  -H "Authorization: Bearer <admin-key>" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-service",
    "permissions": ["read", "write"],
    "scopes": ["*"],
    "expires_at": null
  }'
```

Response:

```json
{
  "key": "sk_live_abc123def456...",
  "key_prefix": "sk_live_abc123",
  "name": "my-service",
  "permissions": ["read", "write"],
  "scopes": ["*"],
  "created_at": "2026-03-06T12:00:00Z"
}
```

Save the full key — it's only shown once. After creation, only the prefix is stored.

### Key Options

| Field | Description | Example |
|-------|-------------|---------|
| `name` | Human-readable label | `"production-writer"` |
| `permissions` | Access level | `["read"]`, `["write"]`, `["read", "write"]`, `["admin"]` |
| `scopes` | Topic patterns (wildcards supported) | `["*"]`, `["orders-*"]`, `["events", "logs"]` |
| `expires_at` | Optional expiration (ISO 8601) | `"2026-12-31T23:59:59Z"` or `null` |

### Permission Levels

| Permission | Allows |
|-----------|--------|
| `read` | Consume messages, list topics, fetch offsets, SQL queries |
| `write` | Produce messages, commit offsets, create topics |
| `admin` | All of the above, plus: delete topics, manage consumer groups, manage connectors, manage API keys |

### Scope Patterns

Scopes use glob-style matching on topic names:

- `["*"]` — All topics
- `["orders-*"]` — Topics starting with `orders-` (e.g., `orders-us`, `orders-eu`)
- `["events", "logs"]` — Exactly `events` and `logs`

A request is allowed if the topic matches **any** scope in the key's scope list.

### Bootstrapping

When you first enable auth, use the `STREAMHOUSE_ADMIN_KEY` environment variable to create your initial organization and API keys:

```bash
# Start the server with an admin key
STREAMHOUSE_AUTH_ENABLED=true STREAMHOUSE_ADMIN_KEY=my-bootstrap-key ./target/release/unified-server

# Create an organization
curl -X POST http://localhost:8080/api/v1/organizations \
  -H "Authorization: Bearer my-bootstrap-key" \
  -H "Content-Type: application/json" \
  -d '{"name": "my-org", "slug": "my-org"}'

# Create an API key for the org
curl -X POST http://localhost:8080/api/v1/organizations/{org_id}/api-keys \
  -H "Authorization: Bearer my-bootstrap-key" \
  -H "Content-Type: application/json" \
  -d '{"name": "production", "permissions": ["read", "write"], "scopes": ["*"]}'
```

The admin key has full access to all organizations and endpoints. Use it only for initial setup and management — create scoped API keys for application use.

### Manage Keys

```bash
# List keys for an org
curl http://localhost:8080/api/v1/organizations/{org_id}/api-keys \
  -H "Authorization: Bearer <admin-key>"

# Revoke a key
curl -X DELETE http://localhost:8080/api/v1/organizations/{org_id}/api-keys/{key_id} \
  -H "Authorization: Bearer <admin-key>"
```

---

## Using Auth by Protocol

### REST — Bearer Token

```bash
curl -H "Authorization: Bearer sk_live_abc123..." \
  http://localhost:8080/api/v1/topics
```

The `SmartAuthLayer` middleware:
1. Extracts the Bearer token
2. SHA-256 hashes it
3. Looks up the hash in the metadata store
4. Resolves the organization
5. Stores the authenticated context in request extensions

All subsequent handlers use the authenticated org — no `X-Organization-Id` header needed.

### Kafka — SASL/PLAIN

Use your API key as both username and password:

```python
from confluent_kafka import Producer

producer = Producer({
    'bootstrap.servers': 'localhost:9092',
    'security.protocol': 'SASL_PLAINTEXT',  # or SASL_SSL with TLS
    'sasl.mechanism': 'PLAIN',
    'sasl.username': 'sk_live_abc123...',
    'sasl.password': 'sk_live_abc123...',
})
```

The SASL handshake resolves the API key to an organization. All subsequent Kafka operations (produce, fetch, consumer groups, transactions) are scoped to that org.

```bash
# kcat with auth
kcat -C -b localhost:9092 -t events \
  -X security.protocol=SASL_PLAINTEXT \
  -X sasl.mechanism=PLAIN \
  -X sasl.username=sk_live_abc123... \
  -X sasl.password=sk_live_abc123...
```

### gRPC — Metadata Header

```python
import grpc

channel = grpc.insecure_channel('localhost:50051')
metadata = [('authorization', 'Bearer sk_live_abc123...')]
response = stub.ListTopics(request, metadata=metadata)
```

---

## Organization Isolation

Every piece of data is scoped to an organization:

- **S3 paths**: `org-{uuid}/data/{topic}/{partition}/{offset}.seg`
- **Metadata**: Topics, consumer groups, connectors, transactions — all org-scoped
- **Cache**: Cache keys include org_id to prevent cross-org reads
- **Metrics**: All Prometheus metrics carry `org_id` as a label

Two organizations can have topics with the same name — they're completely isolated.

---

## Dev Mode (Auth Disabled)

When `STREAMHOUSE_AUTH_ENABLED` is not set or `false`:

- REST requests use `X-Organization-Id` header (falls back to default org)
- Kafka connections use the default organization
- No API key validation

This is the default for local development and Docker Compose.

```bash
# Dev mode — specify org via header
curl -H "X-Organization-Id: my-org-uuid" \
  http://localhost:8080/api/v1/topics
```

---

## Web UI (Clerk)

The managed service Web UI uses Clerk for user authentication. The flow:

1. User signs up / logs in via Clerk on the Web UI
2. Clerk session identifies the user and organization
3. Web UI calls backend REST API with the user's API key
4. Backend validates the key and scopes all operations to the org

API key management is available in the Web UI at `/settings/api-keys`.
