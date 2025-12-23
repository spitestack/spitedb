/**
 * Admin API Handlers
 *
 * Backend handlers for the spite-stage admin dashboard.
 * Provides read-only access to SpiteDB and TelemetryDB for observability.
 */

import type {
  SpiteDbNapi,
  TelemetryDbNapi,
  TelemetryQueryNapi,
  EventNapi,
} from '@spitestack/db';

// =============================================================================
// Types
// =============================================================================

export interface AdminContext {
  db: SpiteDbNapi;
  telemetry: TelemetryDbNapi;
  projectionNames: string[];
  startTime: number;
}

export interface AdminStatusResponse {
  connected: boolean;
  uptime: number;
  version: string;
  environment: string;
  projectionsCount: number;
}

export interface AdminMetricsResponse {
  eventsPerSec: { read: number; write: number };
  totalEvents: number;
  admission: {
    currentLimit: number;
    observedP99Ms: number;
    targetP99Ms: number;
    requestsAccepted: number;
    requestsRejected: number;
    rejectionRate: number;
    adjustments: number;
  };
}

export interface AdminProjectionStatus {
  name: string;
  health: 'healthy' | 'warning' | 'error';
  checkpoint: number;
  lag: number;
  lastProcessed: number;
}

export interface AdminProjectionsResponse {
  projections: AdminProjectionStatus[];
  globalHead: number;
}

export interface AdminLogEntry {
  id: string;
  timestamp: number;
  severity: 'debug' | 'info' | 'warn' | 'error';
  message: string;
  service?: string;
  traceId?: string;
  spanId?: string;
  attrs?: Record<string, unknown>;
}

export interface AdminLogsResponse {
  logs: AdminLogEntry[];
  hasMore: boolean;
}

export interface AdminEventEntry {
  globalPos: number;
  streamId: string;
  streamRev: number;
  timestamp: number;
  tenantHash: number;
  dataPreview: string;
}

export interface AdminEventsResponse {
  events: AdminEventEntry[];
  hasMore: boolean;
  globalHead: number;
}

// =============================================================================
// Response Helpers
// =============================================================================

function jsonResponse<T>(data: T, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function errorResponse(message: string, status = 500): Response {
  return jsonResponse({ error: message }, status);
}

function severityToString(severity: number | null | undefined): AdminLogEntry['severity'] {
  switch (severity) {
    case 0: return 'debug';
    case 1: return 'info';
    case 2: return 'warn';
    case 3: return 'error';
    default: return 'info';
  }
}

function truncateJson(data: Buffer | Uint8Array, maxLength = 500): string {
  try {
    const str = new TextDecoder().decode(data);
    const parsed = JSON.parse(str);
    const pretty = JSON.stringify(parsed);
    if (pretty.length <= maxLength) {
      return pretty;
    }
    return pretty.slice(0, maxLength - 3) + '...';
  } catch {
    return '[binary data]';
  }
}

// =============================================================================
// Handlers
// =============================================================================

/**
 * GET /admin/api/status
 * Returns server status and basic info.
 */
export async function handleAdminStatus(ctx: AdminContext): Promise<Response> {
  const uptime = Math.floor((Date.now() - ctx.startTime) / 1000);

  const response: AdminStatusResponse = {
    connected: true,
    uptime,
    version: '0.1.0', // TODO: Read from package.json or env
    environment: process.env.NODE_ENV ?? 'development',
    projectionsCount: ctx.projectionNames.length,
  };

  return jsonResponse(response);
}

/**
 * GET /admin/api/metrics
 * Returns event throughput and admission control metrics.
 */
export async function handleAdminMetrics(ctx: AdminContext): Promise<Response> {
  try {
    const admission = ctx.db.getAdmissionMetrics();

    // Query recent metrics from telemetry for events/sec
    // Look at the last 5 seconds of http.request.count metrics
    const now = Date.now();
    const query: TelemetryQueryNapi = {
      kind: 'Metric',
      startMs: now - 5000,
      endMs: now,
      metricName: 'http.request.count',
      limit: 100,
      order: 'Desc',
    };

    let readCount = 0;
    let writeCount = 0;

    try {
      const metrics = await ctx.telemetry.query(query);
      for (const record of metrics) {
        if (record.attrsJson) {
          try {
            const attrs = JSON.parse(record.attrsJson);
            if (attrs.method === 'GET') {
              readCount += record.metricValue ?? 0;
            } else if (attrs.method === 'POST') {
              writeCount += record.metricValue ?? 0;
            }
          } catch {
            // Ignore parse errors
          }
        }
      }
    } catch {
      // Telemetry query failed, use zeros
    }

    // Normalize to per-second
    const eventsPerSec = {
      read: Math.round(readCount / 5),
      write: Math.round(writeCount / 5),
    };

    // Get total events by reading global position
    let totalEvents = 0;
    try {
      const events = await ctx.db.readGlobal(0, 1);
      if (events.length > 0) {
        // Read from the end to get the latest position
        const latest = await ctx.db.readGlobal(Number.MAX_SAFE_INTEGER - 1000000, 1);
        if (latest.length > 0) {
          totalEvents = Number(latest[0].globalPos);
        }
      }
    } catch {
      // Ignore read errors
    }

    const response: AdminMetricsResponse = {
      eventsPerSec,
      totalEvents,
      admission: {
        currentLimit: Number(admission.currentLimit),
        observedP99Ms: admission.observedP99Ms,
        targetP99Ms: admission.targetP99Ms,
        requestsAccepted: Number(admission.requestsAccepted),
        requestsRejected: Number(admission.requestsRejected),
        rejectionRate: admission.rejectionRate,
        adjustments: Number(admission.adjustments),
      },
    };

    return jsonResponse(response);
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Failed to get metrics';
    return errorResponse(message);
  }
}

/**
 * GET /admin/api/projections
 * Returns status of all registered projections.
 */
export async function handleAdminProjections(ctx: AdminContext): Promise<Response> {
  try {
    const projections: AdminProjectionStatus[] = [];

    // Get current global head position
    let globalHead = 0;
    try {
      const events = await ctx.db.readGlobal(Number.MAX_SAFE_INTEGER - 1000000, 1);
      if (events.length > 0) {
        globalHead = Number(events[0].globalPos);
      }
    } catch {
      // Ignore
    }

    for (const name of ctx.projectionNames) {
      try {
        const checkpoint = await ctx.db.getProjectionCheckpoint(name);
        const pos = checkpoint ?? 0;
        const lag = globalHead - pos;

        // Determine health based on lag
        let health: AdminProjectionStatus['health'] = 'healthy';
        if (lag > 10000) {
          health = 'error';
        } else if (lag > 1000) {
          health = 'warning';
        }

        projections.push({
          name,
          health,
          checkpoint: pos,
          lag,
          lastProcessed: Date.now(), // TODO: Track actual last processed time
        });
      } catch {
        projections.push({
          name,
          health: 'error',
          checkpoint: 0,
          lag: globalHead,
          lastProcessed: 0,
        });
      }
    }

    const response: AdminProjectionsResponse = {
      projections,
      globalHead,
    };

    return jsonResponse(response);
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Failed to get projections';
    return errorResponse(message);
  }
}

/**
 * GET /admin/api/logs
 * Query telemetry logs with optional filters.
 */
export async function handleAdminLogs(
  ctx: AdminContext,
  searchParams: URLSearchParams
): Promise<Response> {
  try {
    const limit = Math.min(parseInt(searchParams.get('limit') ?? '100', 10), 1000);
    const severity = searchParams.get('severity');
    const startMs = searchParams.get('startMs');
    const endMs = searchParams.get('endMs');
    const service = searchParams.get('service');
    const traceId = searchParams.get('traceId');

    const query: TelemetryQueryNapi = {
      kind: 'Log',
      limit: limit + 1, // +1 to detect hasMore
      order: 'Desc',
    };

    if (severity) {
      const severityMap: Record<string, number> = {
        debug: 0,
        info: 1,
        warn: 2,
        error: 3,
      };
      query.severity = severityMap[severity] ?? 1;
    }

    if (startMs) {
      query.startMs = parseInt(startMs, 10);
    }

    if (endMs) {
      query.endMs = parseInt(endMs, 10);
    }

    if (traceId) {
      query.traceId = traceId;
    }

    const records = await ctx.telemetry.query(query);

    // Filter by service if specified (post-filter since TelemetryDB doesn't have service index)
    const filteredRecords = service
      ? records.filter(r => r.service === service)
      : records;

    const logs: AdminLogEntry[] = filteredRecords.slice(0, limit).map((record, index) => {
      let attrs: Record<string, unknown> | undefined;
      if (record.attrsJson) {
        try {
          attrs = JSON.parse(record.attrsJson);
        } catch {
          // Ignore
        }
      }

      return {
        id: `${record.tsMs}-${index}`,
        timestamp: Number(record.tsMs),
        severity: severityToString(record.severity),
        message: record.message ?? '',
        service: record.service,
        traceId: record.traceId,
        spanId: record.spanId,
        attrs,
      };
    });

    const response: AdminLogsResponse = {
      logs,
      hasMore: filteredRecords.length > limit,
    };

    return jsonResponse(response);
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Failed to get logs';
    return errorResponse(message);
  }
}

/**
 * GET /admin/api/events
 * Read events from the global log.
 */
export async function handleAdminEvents(
  ctx: AdminContext,
  searchParams: URLSearchParams
): Promise<Response> {
  try {
    const limit = Math.min(parseInt(searchParams.get('limit') ?? '50', 10), 500);
    const fromPos = parseInt(searchParams.get('fromPos') ?? '0', 10);
    const direction = searchParams.get('direction') ?? 'forward';

    let events: EventNapi[];
    let globalHead = 0;

    // Get global head position
    try {
      const latest = await ctx.db.readGlobal(Number.MAX_SAFE_INTEGER - 1000000, 1);
      if (latest.length > 0) {
        globalHead = Number(latest[0].globalPos);
      }
    } catch {
      // Ignore
    }

    if (direction === 'backward' && fromPos === 0) {
      // Read from the end (most recent events)
      const startPos = Math.max(0, globalHead - limit);
      events = await ctx.db.readGlobal(startPos, limit + 1);
      events.reverse(); // Show newest first
    } else {
      events = await ctx.db.readGlobal(fromPos, limit + 1);
    }

    const hasMore = events.length > limit;
    const resultEvents = events.slice(0, limit);

    const response: AdminEventsResponse = {
      events: resultEvents.map((event) => ({
        globalPos: Number(event.globalPos),
        streamId: event.streamId,
        streamRev: Number(event.streamRev),
        timestamp: Number(event.timestampMs),
        tenantHash: Number(event.tenantHash),
        dataPreview: truncateJson(event.data),
      })),
      hasMore,
      globalHead,
    };

    return jsonResponse(response);
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Failed to get events';
    return errorResponse(message);
  }
}

/**
 * GET /admin/api/streams/:streamId
 * Read events from a specific stream.
 */
export async function handleAdminStream(
  ctx: AdminContext,
  streamId: string,
  tenant: string,
  searchParams: URLSearchParams
): Promise<Response> {
  try {
    const limit = Math.min(parseInt(searchParams.get('limit') ?? '50', 10), 500);
    const fromRev = parseInt(searchParams.get('fromRev') ?? '0', 10);

    const revision = await ctx.db.getStreamRevision(streamId, tenant);
    const events = await ctx.db.readStream(streamId, fromRev, limit + 1, tenant);

    const hasMore = events.length > limit;
    const resultEvents = events.slice(0, limit);

    return jsonResponse({
      streamId,
      revision: Number(revision),
      events: resultEvents.map((event) => ({
        globalPos: Number(event.globalPos),
        streamRev: Number(event.streamRev),
        timestamp: Number(event.timestampMs),
        dataPreview: truncateJson(event.data),
      })),
      hasMore,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Failed to get stream';
    return errorResponse(message);
  }
}
