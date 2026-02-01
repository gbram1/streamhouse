# StreamHouse Web Console

A comprehensive web dashboard for monitoring and managing StreamHouse streaming infrastructure, built with Next.js 14 and shadcn/ui.

## Features Implemented

### 📊 10 Comprehensive Dashboards

1. **Overview** - System health, throughput, latency, consumer lag with real-time charts
2. **Topics** - Topic management, search, favorites, message browser
3. **Consumers** - Consumer group monitoring, lag tracking, trend indicators
4. **Producers** - Producer monitoring and management
5. **Partitions** - Partition distribution and health
6. **Performance** - 5 interactive charts (throughput, latency, errors)
7. **Storage** - S3/MinIO metrics, WAL status, cache performance with charts
8. **Agents** - Agent health, resource usage, partition assignments
9. **Schemas** - Schema registry with Avro/Protobuf/JSON support
10. **Monitoring** - System-wide monitoring and alerts

### 🎨 UI Components

- **Sidebar Navigation** - Persistent navigation with active route highlighting
- **Theme Support** - Light/Dark/System theme with persistent preferences
- **Responsive Design** - Mobile-friendly layout with Tailwind CSS
- **Interactive Charts** - 4 chart types (Line, Area, Bar, Pie) using Recharts
- **Message Browser** - Full-featured message viewer with search, pagination, detail view
- **Real-time Updates** - WebSocket support for live metric streaming

### 🛠️ Technical Stack

- **Framework**: Next.js 14 (App Router)
- **Language**: TypeScript (strict mode)
- **State Management**: Zustand with persistence
- **Data Fetching**: React Query (@tanstack/react-query)
- **Charts**: Recharts
- **UI Components**: shadcn/ui + Radix UI
- **Styling**: Tailwind CSS v4
- **Icons**: Lucide React

## Getting Started

### Installation

```bash
cd web
npm install
```

### Development

```bash
npm run dev
```

Open [http://localhost:3000](http://localhost:3000) in your browser.

### Environment Variables

Create a `.env.local` file:

```env
NEXT_PUBLIC_API_URL=http://localhost:8080
NEXT_PUBLIC_WS_URL=ws://localhost:8080
```

### Production Build

```bash
npm run build
npm run start
```

## Project Structure

```
web/
├── app/                          # Next.js App Router pages
│   ├── dashboard/                # Overview dashboard
│   ├── topics/                   # Topics list and detail pages
│   │   └── [name]/              # Topic detail with message browser
│   ├── consumers/                # Consumer groups monitoring
│   ├── producers/                # Producer monitoring
│   ├── partitions/               # Partition health
│   ├── performance/              # Performance metrics with charts
│   ├── storage/                  # Storage & cache metrics
│   ├── agents/                   # Agent monitoring
│   ├── schemas/                  # Schema registry
│   └── monitoring/               # System monitoring
├── components/
│   ├── charts/                   # Reusable chart components
│   │   ├── line-chart.tsx       # Time-series line charts
│   │   ├── area-chart.tsx       # Area charts with gradients
│   │   ├── bar-chart.tsx        # Bar charts
│   │   └── pie-chart.tsx        # Pie charts
│   ├── layout/                   # Layout components
│   │   ├── sidebar.tsx          # Navigation sidebar
│   │   ├── header.tsx           # Dashboard header
│   │   └── dashboard-layout.tsx # Unified layout wrapper
│   ├── message-browser.tsx       # Message browsing component
│   └── ui/                       # shadcn/ui components
├── lib/
│   ├── api-client.ts            # Centralized HTTP client
│   ├── store.ts                 # Zustand global state
│   ├── types.ts                 # TypeScript type definitions
│   ├── utils.ts                 # Utility functions
│   ├── query-provider.tsx       # React Query configuration
│   └── hooks/                    # Custom React hooks
│       ├── use-topics.ts        # Topic operations
│       ├── use-consumer-groups.ts
│       ├── use-schemas.ts
│       ├── use-metrics.ts
│       ├── use-websocket.ts     # WebSocket client
│       └── use-realtime-metrics.ts
└── README.md                     # This file
```

## Key Features

### Message Browser
- Search messages by key, value, partition, or offset
- Pagination (50 messages per page)
- Message detail view with JSON formatting
- Copy message value to clipboard
- Export messages to JSON

### Real-time Updates
- WebSocket connection status indicator
- Live metric streaming when auto-refresh enabled
- Configurable time ranges (5m, 15m, 1h, 6h, 24h, 7d, 30d)
- Auto-reconnect with exponential backoff

### Chart Visualizations
- **Performance Dashboard**: 5 charts
  - Message throughput (24h)
  - Latency percentiles (p50, p95, p99)
  - Error rate over time
  - Error types distribution
  - Network throughput
- **Storage Dashboard**: 2 charts
  - Cache hit rate (24h)
  - Cache evictions (12h)
- **Consumers Dashboard**: 1 chart
  - Consumer lag over time (24h, multi-series)
- **Overview Dashboard**: 2 charts
  - Message throughput
  - Consumer lag by group

### Persistent User Preferences
- Theme selection (light/dark/system)
- Favorite topics
- Favorite consumer groups
- Auto-refresh toggle
- Time range selection

## Current Status

### ✅ Completed
- All 10 dashboards with full UI implementation
- 4 reusable chart components (Line, Area, Bar, Pie)
- Message browser with search and pagination
- WebSocket hooks for real-time updates
- Theme provider with persistence
- Sidebar navigation
- React Query data fetching setup
- Comprehensive TypeScript types

### 🚧 Pending Backend Integration

The UI is **fully functional with mock data**. To connect to the real StreamHouse backend, the following REST API endpoints need to be implemented:

#### Required API Endpoints

**Topics**
- `GET /api/v1/topics` - List all topics
- `GET /api/v1/topics/:name` - Get topic details
- `GET /api/v1/topics/:name/messages` - Get topic messages
- `GET /api/v1/topics/:name/partitions` - Get topic partitions
- `POST /api/v1/topics` - Create topic
- `DELETE /api/v1/topics/:name` - Delete topic

**Consumer Groups**
- `GET /api/v1/consumer-groups` - List consumer groups
- `GET /api/v1/consumer-groups/:id` - Get group details
- `GET /api/v1/consumer-groups/:id/lag` - Get consumer lag

**Agents**
- `GET /api/v1/agents` - List all agents
- `GET /api/v1/agents/:id` - Get agent details
- `GET /api/v1/agents/:id/metrics` - Get agent metrics

**Metrics**
- `GET /api/v1/metrics/overview` - System overview metrics
- `GET /api/v1/metrics/throughput` - Throughput metrics
- `GET /api/v1/metrics/latency` - Latency metrics
- `GET /api/v1/metrics/errors` - Error metrics
- `GET /api/v1/metrics/storage` - Storage metrics

**WebSocket**
- `WS /ws/metrics` - Real-time metrics stream
- `WS /ws/topics/:name` - Real-time topic metrics
- `WS /ws/consumers/:id` - Real-time consumer metrics

**Schema Registry** (already implemented in backend)
- `GET /schemas/subjects` - List schema subjects
- `GET /schemas/subjects/:subject/versions` - Get schema versions
- `GET /schemas/ids/:id` - Get schema by ID

## Mock Data

The application currently uses mock data generators for demonstration purposes. Mock data is used for:

- Topic lists and details
- Consumer group lag
- Performance metrics (throughput, latency, errors)
- Storage metrics (cache hit rate, evictions)
- Agent health and resource usage

Mock data is generated in:
- Individual dashboard pages (e.g., `app/performance/page.tsx`)
- Custom hooks (e.g., `lib/hooks/use-metrics.ts`)

To switch to real data, the backend API endpoints listed above need to be implemented.

## Integration Checklist

To connect the web console to a running StreamHouse cluster:

- [ ] Implement REST API endpoints in StreamHouse agent or separate web API service
- [ ] Add CORS headers to allow web console origin
- [ ] Implement WebSocket endpoints for real-time updates
- [ ] Update `.env.local` with correct API URLs
- [ ] Remove or conditionally use mock data generators
- [ ] Test with real StreamHouse cluster

## Architecture Decisions

### Why Mock Data First?
Building the UI with mock data allows rapid iteration on user experience without blocking on backend API development. Once the backend APIs are ready, switching to real data requires minimal changes.

### State Management
- **Zustand**: Simple, performant global state for UI preferences
- **React Query**: Server state caching, automatic refetching, optimistic updates
- **Local State**: Component-specific state (search queries, pagination)

### Chart Library
Recharts was chosen for:
- React-first API
- Responsive by default
- Composable chart components
- Good TypeScript support
- Extensive customization options

### Component Organization
- `components/ui/`: Generic UI primitives (shadcn/ui)
- `components/charts/`: Reusable chart wrappers
- `components/layout/`: Layout components (Sidebar, Header)
- `components/*.tsx`: Feature-specific components (MessageBrowser)

## Performance Considerations

- **Code Splitting**: Next.js automatically splits routes
- **Image Optimization**: Using Next.js Image component
- **React Query Caching**: 5s stale time, 10s refetch interval
- **WebSocket Throttling**: Only connect when auto-refresh enabled
- **Pagination**: Limit table rows to 50 per page
- **Chart Data**: Limit time-series data to 100 points max

## Browser Support

- Chrome/Edge: ✅ Latest 2 versions
- Firefox: ✅ Latest 2 versions
- Safari: ✅ Latest 2 versions

## License

Part of StreamHouse - S3-native event streaming platform.
