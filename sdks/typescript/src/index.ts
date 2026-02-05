/**
 * StreamHouse TypeScript/JavaScript Client SDK
 *
 * A type-safe client for interacting with StreamHouse streaming data platform.
 *
 * @example
 * ```typescript
 * import { StreamHouseClient } from 'streamhouse';
 *
 * const client = new StreamHouseClient('http://localhost:8080');
 * const topics = await client.listTopics();
 * ```
 */

export * from './client';
export * from './types';
export * from './errors';
