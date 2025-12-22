/**
 * Projection Worker
 *
 * Entry point for projection worker threads.
 * Each worker is fully autonomous - it owns its projection database,
 * polls for events, runs the apply function, and writes results.
 *
 * Workers are spawned by ProjectionRunner.startAll().
 */

/// <reference types="bun-types" />

import { workerData } from 'worker_threads';
import { __getProjectionDefinition, type ProjectionDefinition } from './define';
import { createProjectionProxy } from './proxy';
import type { ColumnType, ProjectionEvent, SchemaDefinition } from './types';

/**
 * Configuration passed to the worker via workerData.
 */
interface WorkerConfig {
  /** Path to the projection module */
  modulePath: string;
  /** Path to the event store database */
  eventStorePath: string;
  /** Directory for projection databases */
  projectionsDir: string;
}

/**
 * Column definition for NAPI.
 */
interface ColumnDefNapi {
  name: string;
  colType: string;
  primaryKey: boolean;
  nullable: boolean;
  defaultValue?: string;
}

/**
 * Event from NAPI.
 */
interface EventNapi {
  globalPos: number;
  streamId: string;
  tenantHash: number;
  streamRev: number;
  timestampMs: number;
  data: Buffer;
}

/**
 * Event batch from NAPI.
 */
interface EventBatchNapi {
  projectionName: string;
  events: EventNapi[];
  batchId: number;
}

/**
 * Converts a schema definition to NAPI column definitions.
 */
function schemaToColumnDefs(schema: SchemaDefinition): ColumnDefNapi[] {
  const columns: ColumnDefNapi[] = [];

  for (const [name, def] of Object.entries(schema)) {
    if (typeof def === 'string') {
      // Simple type definition
      columns.push({
        name,
        colType: def,
        primaryKey: false,
        nullable: true,
        defaultValue: undefined,
      });
    } else {
      // Full definition
      columns.push({
        name,
        colType: def.type,
        primaryKey: def.primaryKey ?? false,
        nullable: def.nullable ?? true,
        defaultValue: def.defaultValue !== undefined
          ? JSON.stringify(def.defaultValue)
          : undefined,
      });
    }
  }

  return columns;
}

/**
 * Finds the primary key column name from a schema.
 */
function findPrimaryKey(schema: SchemaDefinition): string {
  for (const [name, def] of Object.entries(schema)) {
    if (typeof def === 'object' && def.primaryKey) {
      return name;
    }
  }
  // Default to first column if no explicit primary key
  return Object.keys(schema)[0];
}

/**
 * Converts a NAPI event to a ProjectionEvent.
 */
function napiEventToProjectionEvent(event: EventNapi): ProjectionEvent {
  return {
    globalPos: BigInt(event.globalPos),
    streamId: event.streamId,
    tenantHash: BigInt(event.tenantHash),
    streamRev: BigInt(event.streamRev),
    timestampMs: BigInt(event.timestampMs),
    data: event.data,
  };
}

/**
 * Main worker function.
 */
async function main(): Promise<void> {
  const config = workerData as WorkerConfig;

  // Import the projection module
  // This will execute defineProjection() and populate __currentDefinition
  await import(config.modulePath);

  // Retrieve the projection definition
  const projection = __getProjectionDefinition();

  // Validate the projection
  if (!projection) {
    throw new Error(
      `Projection module must use defineProjection() and export default.\n` +
      `File: ${config.modulePath}\n\n` +
      `Example:\n` +
      `  import { defineProjection } from 'spitedb';\n\n` +
      `  export default defineProjection(import.meta.path, {\n` +
      `    name: 'my_projection',\n` +
      `    schema: { id: { type: 'text', primaryKey: true }, ... },\n` +
      `    apply(event, table) { ... }\n` +
      `  });`
    );
  }

  if (typeof projection.apply !== 'function') {
    throw new Error(
      `Projection "${projection.name}" missing apply function.\n` +
      `defineProjection() requires an apply(event, table) function.`
    );
  }

  if (!projection.schema) {
    throw new Error(
      `Projection "${projection.name}" missing schema.\n` +
      `defineProjection() requires a schema definition.`
    );
  }

  if (!projection.name) {
    throw new Error(
      `Projection missing name.\n` +
      `defineProjection() requires a name property.`
    );
  }

  if (typeof projection.getTenantId !== 'function') {
    throw new Error(
      `Projection "${projection.name}" missing getTenantId function.\n` +
      `defineProjection() requires a getTenantId(event) function for tenant isolation.\n\n` +
      `Example:\n` +
      `  getTenantId: (event) => event.streamId.split('-')[1]`
    );
  }

  // Load NAPI bindings
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { SpiteDBNapi } = require('../index.js');

  // Open the event store
  const db = await SpiteDBNapi.open(config.eventStorePath);

  // Initialize this projection's database
  await db.initProjections(config.projectionsDir);
  await db.registerProjection(
    projection.name,
    schemaToColumnDefs(projection.schema)
  );

  const batchSize = projection.batchSize ?? 100;
  const pollInterval = projection.pollIntervalMs ?? 50;
  const primaryKey = findPrimaryKey(projection.schema);

  console.log(`[Projection ${projection.name}] Worker started`);

  // Main processing loop
  while (true) {
    try {
      const batch: EventBatchNapi | null = await db.getProjectionEvents(
        projection.name,
        batchSize
      );

      if (!batch || batch.events.length === 0) {
        // No events available, wait and poll again
        await Bun.sleep(pollInterval);
        continue;
      }

      // Track proxies per tenant for this batch
      const tenantProxies = new Map<string, {
        proxy: ReturnType<typeof createProjectionProxy>['proxy'];
        flush: ReturnType<typeof createProjectionProxy>['flush'];
      }>();

      // Get or create proxy for a tenant
      const projName = projection.name; // Capture for closure
      function getProxyForTenant(tenantId: string) {
        let proxyData = tenantProxies.get(tenantId);
        if (!proxyData) {
          const { proxy, flush } = createProjectionProxy(
            projName,
            tenantId,
            primaryKey,
            db
          );
          proxyData = { proxy, flush };
          tenantProxies.set(tenantId, proxyData);
        }
        return proxyData.proxy;
      }

      let lastGlobalPos = batch.batchId;

      // Process each event with its tenant's proxy
      for (const eventNapi of batch.events) {
        const event = napiEventToProjectionEvent(eventNapi);

        try {
          // Extract tenant ID from event
          const tenantId = projection.getTenantId(event);
          const proxy = getProxyForTenant(tenantId);

          await projection.apply(event, proxy as never);
          lastGlobalPos = eventNapi.globalPos;
        } catch (error) {
          // Handle error based on strategy
          const strategy = projection.onError?.(error as Error, event) ?? 'stop';

          switch (strategy) {
            case 'skip':
              // Skip this event and continue
              lastGlobalPos = eventNapi.globalPos;
              continue;
            case 'retry':
              // Retry once
              try {
                const tenantId = projection.getTenantId(event);
                const proxy = getProxyForTenant(tenantId);
                await projection.apply(event, proxy as never);
                lastGlobalPos = eventNapi.globalPos;
              } catch {
                // If retry fails, stop
                throw error;
              }
              break;
            case 'stop':
            default:
              throw error;
          }
        }
      }

      // Flush operations from all tenant proxies and apply to database
      for (const [tenantId, proxyData] of tenantProxies) {
        const operations = proxyData.flush();

        if (operations.length > 0) {
          await db.applyProjectionBatch({
            projectionName: projection.name,
            tenantId,
            operations: operations.map((op) => ({
              opType: op.opType,
              key: op.key,
              value: op.value,
            })),
            lastGlobalPos,
          });
        }
      }
    } catch (error) {
      console.error(`[Projection ${projection.name}] Error:`, error);
      // Let the worker crash - the runner will restart it with backoff
      throw error;
    }
  }
}

// Run the worker
main().catch((error) => {
  console.error('Worker fatal error:', error);
  process.exit(1);
});
