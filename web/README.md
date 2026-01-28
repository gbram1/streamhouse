# StreamHouse Web Console

Modern web interface for managing StreamHouse clusters, built with Next.js 15 and shadcn/ui.

## Features

- **Dashboard**: Real-time overview of topics, agents, throughput, and storage
- **Topic Management**: Create, view, and monitor topics and partitions
- **Agent Monitoring**: View agent health, leases, and availability zones
- **Consumer Groups**: Monitor consumer lag and offsets
- **Web Console**: Interactive producer/consumer for testing (coming soon)

## Tech Stack

- **Next.js 15**: React framework with App Router
- **TypeScript**: Type-safe development
- **Tailwind CSS 4**: Utility-first styling
- **shadcn/ui**: High-quality React components
- **lucide-react**: Beautiful icons

## Getting Started

### Development

```bash
# Install dependencies
npm install

# Run development server
npm run dev

# Open http://localhost:3000
```

### Environment Variables

Create `.env.local`:

```bash
# StreamHouse API URL (REST API backend - Phase 10)
NEXT_PUBLIC_API_URL=http://localhost:3001
```

### Build for Production

```bash
# Build
npm run build

# Start production server
npm start
```

## Project Structure

```
web/
├── app/
│   ├── page.tsx              # Landing page
│   ├── dashboard/            # Main dashboard
│   ├── topics/               # Topic management
│   ├── agents/               # Agent monitoring
│   └── console/              # Web console (producer/consumer)
├── components/
│   ├── ui/                   # shadcn/ui components
│   └── layout/               # Shared layout components
├── lib/
│   ├── api/                  # API client
│   └── utils.ts              # Utilities
└── public/                   # Static assets
```

## API Integration

The web console communicates with the StreamHouse REST API backend (to be implemented in Phase 10).

### API Client

TypeScript API client located at `lib/api/client.ts`:

```typescript
import { apiClient } from '@/lib/api/client';

// List topics
const topics = await apiClient.listTopics();

// Create topic
await apiClient.createTopic('orders', 6, 1);

// Get metrics
const metrics = await apiClient.getMetrics();
```

### Mock Data

Currently uses mock data for development. Will connect to real API once REST backend is implemented.

## Phase 10 Integration

This web console is part of **Phase 10: REST API + Web Console** from the production roadmap:

### Week 11-12: REST API Backend (Rust)
- Axum-based HTTP server
- REST endpoints for topics, agents, partitions
- OpenAPI/Swagger docs
- CORS configuration

### Week 13-18: Web Console Features
- **Week 13**: Dashboard with real-time metrics
- **Week 14**: Topic management (create, delete, configure)
- **Week 15**: Agent monitoring and health checks
- **Week 16**: Consumer group monitoring
- **Week 17**: Web-based producer/consumer
- **Week 18**: Authentication and multi-tenancy

## Components

### shadcn/ui Components Used

- **Button**: Actions and navigation
- **Card**: Content containers
- **Table**: Data display
- **Badge**: Status indicators
- **Input/Label**: Form controls
- **Select**: Dropdowns
- **Dialog**: Modals
- **Dropdown Menu**: Context menus
- **Separator**: Visual dividers
- **Tabs**: Content organization
- **Avatar**: User display (coming soon)

### Adding New Components

```bash
# Add shadcn components
npx shadcn@latest add [component-name]

# Example: Add chart component
npx shadcn@latest add chart
```

## Styling

Uses Tailwind CSS 4 with shadcn/ui theme:

- **Primary Color**: Blue (configurable via CSS variables)
- **Theme**: Neutral gray scale
- **Dark Mode**: Automatic via `dark:` prefix

### Customization

Edit `app/globals.css` to customize theme colors:

```css
@theme {
  --color-primary: oklch(0.5 0.2 250);
  --color-secondary: oklch(0.7 0.15 280);
}
```

## Development Notes

### TypeScript Strict Mode

Project uses strict TypeScript. All API types are defined in `lib/api/client.ts`.

### Server Components

Most pages are Server Components for optimal performance. Use `"use client"` only when needed (forms, state, events).

### Data Fetching

Uses React Server Components for data fetching:

```typescript
// app/topics/page.tsx
export default async function TopicsPage() {
  const topics = await apiClient.listTopics();
  return <TopicsList topics={topics} />;
}
```

## Deployment

### Docker

```dockerfile
FROM node:20-alpine AS base

# Install dependencies
FROM base AS deps
WORKDIR /app
COPY package*.json ./
RUN npm ci

# Build
FROM base AS builder
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .
RUN npm run build

# Production
FROM base AS runner
WORKDIR /app
ENV NODE_ENV=production
COPY --from=builder /app/public ./public
COPY --from=builder /app/.next/standalone ./
COPY --from=builder /app/.next/static ./.next/static
EXPOSE 3000
CMD ["node", "server.js"]
```

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: streamhouse-web
spec:
  replicas: 2
  template:
    spec:
      containers:
      - name: web
        image: streamhouse/web:latest
        ports:
        - containerPort: 3000
        env:
        - name: NEXT_PUBLIC_API_URL
          value: "http://streamhouse-api:3001"
```

## Next Steps

1. **Implement REST API Backend** (Phase 10, Week 11-12)
   - Create `crates/streamhouse-api` Rust crate
   - Axum HTTP server
   - REST endpoints
   - OpenAPI docs

2. **Connect to Real API** (Week 13)
   - Replace mock data with API calls
   - Add loading states
   - Error handling

3. **Real-time Updates** (Week 14)
   - WebSocket for live metrics
   - Auto-refresh dashboard
   - Toast notifications

4. **Authentication** (Week 18)
   - User login/signup
   - JWT tokens
   - Protected routes
   - Multi-tenant isolation

## License

Part of StreamHouse - S3-native event streaming platform.
