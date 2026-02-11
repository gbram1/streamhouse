/**
 * StreamHouse Express.js Integration
 *
 * Provides Express-specific middleware, routers, and background consumer
 * utilities for integrating StreamHouse into Express applications.
 *
 * @example
 * ```typescript
 * import express from 'express';
 * import {
 *   streamhouseMiddleware,
 *   createStreamHouseRouter,
 *   BackgroundConsumer,
 *   StreamHousePlugin,
 * } from 'streamhouse/frameworks/express';
 *
 * const app = express();
 *
 * // Auto-publish request events
 * app.use(streamhouseMiddleware({
 *   baseUrl: 'http://localhost:8080',
 *   topic: 'http-requests',
 * }));
 *
 * // Add health/produce/consume REST endpoints
 * app.use('/streamhouse', createStreamHouseRouter({
 *   baseUrl: 'http://localhost:8080',
 * }));
 *
 * // Background consumer with event emitter
 * const consumer = new BackgroundConsumer({
 *   baseUrl: 'http://localhost:8080',
 *   topic: 'events',
 *   partition: 0,
 * });
 * consumer.on('message', (record) => console.log(record));
 * consumer.start();
 *
 * // Or use the plugin for full setup
 * const plugin = new StreamHousePlugin({
 *   baseUrl: 'http://localhost:8080',
 * });
 * plugin.register(app);
 * ```
 */

import { StreamHouseClient } from '../client';
import type {
  StreamHouseClientOptions,
  ConsumedRecord,
} from '../types';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** Options for the StreamHouse Express middleware. */
export interface StreamHouseMiddlewareOptions {
  /** StreamHouse server base URL. */
  baseUrl: string;
  /** Topic to publish request events to. Default: "http-requests". */
  topic?: string;
  /** Optional API key for authentication. */
  apiKey?: string;
  /** Whether to include request headers in events. Default: false. */
  includeHeaders?: boolean;
  /** Custom fetch implementation (for Node.js < 18). */
  fetch?: typeof fetch;
}

/** Options for the StreamHouse Express router. */
export interface StreamHouseRouterOptions {
  /** StreamHouse server base URL. */
  baseUrl: string;
  /** Optional API key for authentication. */
  apiKey?: string;
  /** Custom fetch implementation. */
  fetch?: typeof fetch;
}

/** Options for the BackgroundConsumer. */
export interface BackgroundConsumerOptions {
  /** StreamHouse server base URL. */
  baseUrl: string;
  /** Topic to consume from. */
  topic: string;
  /** Partition to consume from. Default: 0. */
  partition?: number;
  /** Optional API key for authentication. */
  apiKey?: string;
  /** Seconds between poll attempts. Default: 1.0. */
  pollIntervalMs?: number;
  /** Maximum records per poll. Default: 100. */
  maxRecords?: number;
  /** Initial offset to consume from. Default: 0. */
  startOffset?: number;
  /** Custom fetch implementation. */
  fetch?: typeof fetch;
}

/** Options for the StreamHousePlugin. */
export interface StreamHousePluginOptions {
  /** StreamHouse server base URL. */
  baseUrl: string;
  /** Optional API key for authentication. */
  apiKey?: string;
  /** Enable request event middleware. Default: true. */
  enableMiddleware?: boolean;
  /** Topic for request events. Default: "http-requests". */
  middlewareTopic?: string;
  /** Mount path for StreamHouse router. Default: "/streamhouse". */
  routerPath?: string;
  /** Custom fetch implementation. */
  fetch?: typeof fetch;
}

// Minimal Express type definitions to avoid hard dependency on @types/express
interface Request {
  method: string;
  path: string;
  url: string;
  headers: Record<string, string | string[] | undefined>;
  query: Record<string, string | string[] | undefined>;
  body?: unknown;
}

interface Response {
  statusCode: number;
  status(code: number): Response;
  json(body: unknown): Response;
  on(event: string, listener: (...args: unknown[]) => void): Response;
}

type NextFunction = (err?: unknown) => void;

type RequestHandler = (req: Request, res: Response, next: NextFunction) => void;

interface Router {
  get(path: string, ...handlers: RequestHandler[]): Router;
  post(path: string, ...handlers: RequestHandler[]): Router;
}

// ---------------------------------------------------------------------------
// Event Emitter (minimal implementation to avoid Node.js dependency)
// ---------------------------------------------------------------------------

type EventListener = (...args: unknown[]) => void;

class SimpleEventEmitter {
  private listeners: Map<string, EventListener[]> = new Map();

  on(event: string, listener: EventListener): this {
    const existing = this.listeners.get(event) ?? [];
    existing.push(listener);
    this.listeners.set(event, existing);
    return this;
  }

  off(event: string, listener: EventListener): this {
    const existing = this.listeners.get(event);
    if (existing) {
      this.listeners.set(
        event,
        existing.filter((l) => l !== listener)
      );
    }
    return this;
  }

  protected emit(event: string, ...args: unknown[]): boolean {
    const listeners = this.listeners.get(event);
    if (!listeners || listeners.length === 0) return false;
    for (const listener of listeners) {
      listener(...args);
    }
    return true;
  }
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/**
 * Express middleware that automatically publishes HTTP request events to StreamHouse.
 *
 * For each incoming request, publishes an event containing method, path, status code,
 * and duration to the configured topic. Publishing failures are silently caught and
 * logged to avoid disrupting the request/response cycle.
 *
 * @param options - Middleware configuration.
 * @returns An Express middleware function.
 *
 * @example
 * ```typescript
 * import express from 'express';
 * import { streamhouseMiddleware } from 'streamhouse/frameworks/express';
 *
 * const app = express();
 * app.use(streamhouseMiddleware({
 *   baseUrl: 'http://localhost:8080',
 *   topic: 'http-requests',
 *   apiKey: 'sk_live_xxx',
 * }));
 * ```
 */
export function streamhouseMiddleware(
  options: StreamHouseMiddlewareOptions
): RequestHandler {
  const client = new StreamHouseClient({
    baseUrl: options.baseUrl,
    apiKey: options.apiKey,
    fetch: options.fetch,
  });

  const topic = options.topic ?? 'http-requests';
  const includeHeaders = options.includeHeaders ?? false;

  return (req: Request, res: Response, next: NextFunction): void => {
    const startTime = Date.now();

    // Hook into response finish to capture status code and duration
    res.on('finish', () => {
      const event: Record<string, unknown> = {
        method: req.method,
        path: req.path,
        status_code: res.statusCode,
        duration_ms: Date.now() - startTime,
        timestamp: Date.now(),
      };

      if (includeHeaders) {
        event.headers = req.headers;
      }

      // Fire-and-forget: don't await, don't block
      client.produce(topic, JSON.stringify(event)).catch((err) => {
        // Silently ignore publish failures
        if (typeof console !== 'undefined') {
          console.warn('[streamhouse] Failed to publish request event:', err);
        }
      });
    });

    next();
  };
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/**
 * Create an Express router with StreamHouse health, produce, and consume endpoints.
 *
 * Provides a set of REST endpoints that proxy to the StreamHouse API, useful for
 * exposing StreamHouse operations through your application's Express server.
 *
 * Endpoints:
 * - `GET /health` - StreamHouse health check
 * - `POST /produce` - Produce a message (`{ topic, value, key? }`)
 * - `GET /consume` - Consume messages (`?topic=&partition=&offset=&maxRecords=`)
 * - `GET /topics` - List all topics
 *
 * @param options - Router configuration.
 * @returns An Express router (requires `express.Router()` to be available).
 *
 * @example
 * ```typescript
 * import express from 'express';
 * import { createStreamHouseRouter } from 'streamhouse/frameworks/express';
 *
 * const app = express();
 * app.use('/streamhouse', createStreamHouseRouter({
 *   baseUrl: 'http://localhost:8080',
 * }));
 * // GET /streamhouse/health
 * // POST /streamhouse/produce
 * // GET /streamhouse/consume?topic=events&partition=0
 * // GET /streamhouse/topics
 * ```
 */
export function createStreamHouseRouter(options: StreamHouseRouterOptions): Router {
  // Dynamically require express to avoid hard dependency
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const express = require('express');
  const router: Router = express.Router();

  const client = new StreamHouseClient({
    baseUrl: options.baseUrl,
    apiKey: options.apiKey,
    fetch: options.fetch,
  });

  // Health check endpoint
  router.get('/health', async (_req: Request, res: Response) => {
    try {
      const healthy = await client.healthCheck();
      res.status(healthy ? 200 : 503).json({ status: healthy ? 'ok' : 'unavailable' });
    } catch {
      res.status(503).json({ status: 'error' });
    }
  });

  // List topics
  router.get('/topics', async (_req: Request, res: Response) => {
    try {
      const topics = await client.listTopics();
      res.json(topics);
    } catch (err) {
      res.status(500).json({ error: String(err) });
    }
  });

  // Produce a message
  router.post('/produce', async (req: Request, res: Response) => {
    try {
      const { topic, value, key, partition } = req.body as Record<string, string>;
      const result = await client.produce(topic, value, {
        key,
        partition: partition !== undefined ? Number(partition) : undefined,
      });
      res.json(result);
    } catch (err) {
      res.status(500).json({ error: String(err) });
    }
  });

  // Consume messages
  router.get('/consume', async (req: Request, res: Response) => {
    try {
      const topic = String(req.query.topic ?? '');
      const partition = Number(req.query.partition ?? 0);
      const offset = req.query.offset !== undefined ? Number(req.query.offset) : undefined;
      const maxRecords =
        req.query.maxRecords !== undefined ? Number(req.query.maxRecords) : undefined;

      const result = await client.consume(topic, partition, {
        offset,
        maxRecords,
      });
      res.json(result);
    } catch (err) {
      res.status(500).json({ error: String(err) });
    }
  });

  return router;
}

// ---------------------------------------------------------------------------
// Background Consumer
// ---------------------------------------------------------------------------

/**
 * Background consumer that polls StreamHouse and emits events.
 *
 * Runs a continuous consume loop in the background using `setInterval`,
 * emitting events for each consumed message. Supports start/stop lifecycle
 * and automatic offset tracking.
 *
 * Events:
 * - `"message"` - Emitted for each consumed record (payload: `ConsumedRecord`)
 * - `"error"` - Emitted on consume errors (payload: `Error`)
 * - `"started"` - Emitted when the consumer starts
 * - `"stopped"` - Emitted when the consumer stops
 *
 * @example
 * ```typescript
 * import { BackgroundConsumer } from 'streamhouse/frameworks/express';
 *
 * const consumer = new BackgroundConsumer({
 *   baseUrl: 'http://localhost:8080',
 *   topic: 'events',
 *   partition: 0,
 *   pollIntervalMs: 2000,
 * });
 *
 * consumer.on('message', (record) => {
 *   console.log(`Received: ${record.value} at offset ${record.offset}`);
 * });
 *
 * consumer.on('error', (err) => {
 *   console.error('Consumer error:', err);
 * });
 *
 * await consumer.start();
 *
 * // Later...
 * await consumer.stop();
 * ```
 */
export class BackgroundConsumer extends SimpleEventEmitter {
  private client: StreamHouseClient;
  private readonly topic: string;
  private readonly partition: number;
  private readonly pollIntervalMs: number;
  private readonly maxRecords: number;
  private currentOffset: number;
  private running: boolean = false;
  private pollTimer: ReturnType<typeof setInterval> | null = null;

  constructor(options: BackgroundConsumerOptions) {
    super();
    this.client = new StreamHouseClient({
      baseUrl: options.baseUrl,
      apiKey: options.apiKey,
      fetch: options.fetch,
    });
    this.topic = options.topic;
    this.partition = options.partition ?? 0;
    this.pollIntervalMs = options.pollIntervalMs ?? 1000;
    this.maxRecords = options.maxRecords ?? 100;
    this.currentOffset = options.startOffset ?? 0;
  }

  /**
   * Start the background consumer.
   *
   * Begins polling StreamHouse at the configured interval.
   */
  async start(): Promise<void> {
    if (this.running) return;
    this.running = true;
    this.emit('started');

    // Run first poll immediately, then schedule recurring polls
    await this.poll();
    this.pollTimer = setInterval(() => {
      this.poll().catch((err) => this.emit('error', err));
    }, this.pollIntervalMs);
  }

  /**
   * Stop the background consumer.
   */
  async stop(): Promise<void> {
    if (!this.running) return;
    this.running = false;

    if (this.pollTimer !== null) {
      clearInterval(this.pollTimer);
      this.pollTimer = null;
    }

    this.emit('stopped');
  }

  /**
   * Get the current consumer offset.
   */
  getOffset(): number {
    return this.currentOffset;
  }

  /**
   * Check if the consumer is currently running.
   */
  isRunning(): boolean {
    return this.running;
  }

  private async poll(): Promise<void> {
    if (!this.running) return;

    try {
      const result = await this.client.consume(this.topic, this.partition, {
        offset: this.currentOffset,
        maxRecords: this.maxRecords,
      });

      for (const record of result.records) {
        this.emit('message', record);
        this.currentOffset = record.offset + 1;
      }
    } catch (err) {
      this.emit('error', err);
    }
  }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/**
 * All-in-one StreamHouse plugin for Express applications.
 *
 * Combines middleware, router, and client into a single configuration point.
 * Call `register(app)` to set up all StreamHouse integrations on your Express app.
 *
 * @example
 * ```typescript
 * import express from 'express';
 * import { StreamHousePlugin } from 'streamhouse/frameworks/express';
 *
 * const app = express();
 * const plugin = new StreamHousePlugin({
 *   baseUrl: 'http://localhost:8080',
 *   enableMiddleware: true,
 *   middlewareTopic: 'http-requests',
 *   routerPath: '/streamhouse',
 * });
 *
 * plugin.register(app);
 *
 * // Access the client
 * const topics = await plugin.getClient().listTopics();
 * ```
 */
export class StreamHousePlugin {
  private readonly options: StreamHousePluginOptions;
  private client: StreamHouseClient;

  constructor(options: StreamHousePluginOptions) {
    this.options = options;
    this.client = new StreamHouseClient({
      baseUrl: options.baseUrl,
      apiKey: options.apiKey,
      fetch: options.fetch,
    });
  }

  /**
   * Register the plugin with an Express app.
   *
   * @param app - Express application instance. Must support `use()`.
   */
  register(app: { use: (...args: unknown[]) => void }): void {
    // Add middleware if enabled
    if (this.options.enableMiddleware !== false) {
      app.use(
        streamhouseMiddleware({
          baseUrl: this.options.baseUrl,
          apiKey: this.options.apiKey,
          topic: this.options.middlewareTopic,
          fetch: this.options.fetch,
        })
      );
    }

    // Mount router
    const routerPath = this.options.routerPath ?? '/streamhouse';
    app.use(
      routerPath,
      createStreamHouseRouter({
        baseUrl: this.options.baseUrl,
        apiKey: this.options.apiKey,
        fetch: this.options.fetch,
      })
    );
  }

  /**
   * Get the underlying StreamHouse client.
   */
  getClient(): StreamHouseClient {
    return this.client;
  }
}
