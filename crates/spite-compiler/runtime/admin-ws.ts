/**
 * Admin WebSocket Handler
 *
 * Real-time streaming for the spite-stage admin dashboard.
 * Supports subscription-based channels for metrics, logs, projections, and events.
 */

import type {
  SpiteDbNapi,
  TelemetryDbNapi,
  TelemetryCursorNapi,
} from '@spitestack/db';

// =============================================================================
// Types
// =============================================================================

export interface AdminWsContext {
  db: SpiteDbNapi;
  telemetry: TelemetryDbNapi;
  projectionNames: string[];
  startTime: number;
}

type Channel = 'metrics' | 'logs' | 'projections' | 'events';

interface ClientMessage {
  type: 'subscribe' | 'unsubscribe';
  channels: Channel[];
}

interface ServerMessage {
  type: 'connected' | 'subscribed' | 'unsubscribed' | 'metrics' | 'log' | 'projection' | 'event' | 'error';
  data?: unknown;
  channels?: Channel[];
  message?: string;
}

interface ConnectionState {
  subscriptions: Set<Channel>;
  logCursor: TelemetryCursorNapi | null;
  lastEventPos: number;
  metricsInterval: ReturnType<typeof setInterval> | null;
  logsInterval: ReturnType<typeof setInterval> | null;
  eventsInterval: ReturnType<typeof setInterval> | null;
  projectionsInterval: ReturnType<typeof setInterval> | null;
}

// Bun's ServerWebSocket type - use generic to work with any data type
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type BunWebSocket = { send(data: string | ArrayBuffer | ArrayBufferView): number; };

// Store connection state per WebSocket
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const connectionStates = new WeakMap<any, ConnectionState>();

// =============================================================================
// Helpers
// =============================================================================

function send(ws: BunWebSocket, message: ServerMessage): void {
  try {
    ws.send(JSON.stringify(message));
  } catch {
    // Connection may be closed
  }
}

function getState(ws: BunWebSocket): ConnectionState {
  let state = connectionStates.get(ws);
  if (!state) {
    state = {
      subscriptions: new Set(),
      logCursor: null,
      lastEventPos: 0,
      metricsInterval: null,
      logsInterval: null,
      eventsInterval: null,
      projectionsInterval: null,
    };
    connectionStates.set(ws, state);
  }
  return state;
}

function clearIntervals(state: ConnectionState): void {
  if (state.metricsInterval) {
    clearInterval(state.metricsInterval);
    state.metricsInterval = null;
  }
  if (state.logsInterval) {
    clearInterval(state.logsInterval);
    state.logsInterval = null;
  }
  if (state.eventsInterval) {
    clearInterval(state.eventsInterval);
    state.eventsInterval = null;
  }
  if (state.projectionsInterval) {
    clearInterval(state.projectionsInterval);
    state.projectionsInterval = null;
  }
}

// =============================================================================
// Channel Handlers
// =============================================================================

async function pushMetrics(ws: BunWebSocket, ctx: AdminWsContext): Promise<void> {
  try {
    const admission = ctx.db.getAdmissionMetrics();

    // Get total events count
    let totalEvents = 0;
    try {
      const latest = await ctx.db.readGlobal(Number.MAX_SAFE_INTEGER - 1000000, 1);
      if (latest.length > 0) {
        totalEvents = Number(latest[0].globalPos);
      }
    } catch {
      // Ignore
    }

    send(ws, {
      type: 'metrics',
      data: {
        eventsPerSec: { read: 0, write: 0 }, // TODO: Calculate from telemetry
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
      },
    });
  } catch (err) {
    send(ws, {
      type: 'error',
      message: err instanceof Error ? err.message : 'Failed to get metrics',
    });
  }
}

async function pushLogs(ws: BunWebSocket, ctx: AdminWsContext, state: ConnectionState): Promise<void> {
  try {
    // Initialize cursor on first call
    if (!state.logCursor) {
      // Start from now (we'll tail forward)
      const now = new Date();
      const slice = `${now.getUTCFullYear()}-${String(now.getUTCMonth() + 1).padStart(2, '0')}-${String(now.getUTCDate()).padStart(2, '0')}`;
      state.logCursor = {
        slice,
        lastIds: [],
      };
    }

    const result = await ctx.telemetry.tail(state.logCursor, 50);

    // Update cursor for next call
    state.logCursor = result.cursor;

    // Push each new log entry
    for (const record of result.records) {
      if (record.kind !== 'Log') continue;

      let attrs: Record<string, unknown> | undefined;
      if (record.attrsJson) {
        try {
          attrs = JSON.parse(record.attrsJson);
        } catch {
          // Ignore
        }
      }

      send(ws, {
        type: 'log',
        data: {
          id: `${record.tsMs}-${record.spanId ?? crypto.randomUUID()}`,
          timestamp: Number(record.tsMs),
          severity: severityToString(record.severity),
          message: record.message ?? '',
          service: record.service,
          traceId: record.traceId,
          spanId: record.spanId,
          attrs,
        },
      });
    }
  } catch {
    // Telemetry tail may fail if no data, that's ok
  }
}

function severityToString(severity: number | null | undefined): string {
  switch (severity) {
    case 0: return 'debug';
    case 1: return 'info';
    case 2: return 'warn';
    case 3: return 'error';
    default: return 'info';
  }
}

async function pushEvents(ws: BunWebSocket, ctx: AdminWsContext, state: ConnectionState): Promise<void> {
  try {
    // Get current head
    let globalHead = 0;
    try {
      const latest = await ctx.db.readGlobal(Number.MAX_SAFE_INTEGER - 1000000, 1);
      if (latest.length > 0) {
        globalHead = Number(latest[0].globalPos);
      }
    } catch {
      return;
    }

    // Initialize position on first call (start from head - we'll see new events)
    if (state.lastEventPos === 0) {
      state.lastEventPos = globalHead;
      return;
    }

    // Read any new events since last check
    if (globalHead > state.lastEventPos) {
      const events = await ctx.db.readGlobal(state.lastEventPos + 1, 100);

      for (const event of events) {
        const pos = Number(event.globalPos);
        if (pos > state.lastEventPos) {
          state.lastEventPos = pos;
        }

        send(ws, {
          type: 'event',
          data: {
            globalPos: pos,
            streamId: event.streamId,
            streamRev: Number(event.streamRev),
            timestamp: Number(event.timestampMs),
            tenantHash: Number(event.tenantHash),
            dataPreview: truncateJson(event.data),
          },
        });
      }
    }
  } catch {
    // Ignore read errors
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

async function pushProjections(ws: BunWebSocket, ctx: AdminWsContext): Promise<void> {
  try {
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
        let health: 'healthy' | 'warning' | 'error' = 'healthy';
        if (lag > 10000) {
          health = 'error';
        } else if (lag > 1000) {
          health = 'warning';
        }

        send(ws, {
          type: 'projection',
          data: {
            name,
            health,
            checkpoint: pos,
            lag,
            globalHead,
            lastProcessed: Date.now(),
          },
        });
      } catch {
        send(ws, {
          type: 'projection',
          data: {
            name,
            health: 'error',
            checkpoint: 0,
            lag: globalHead,
            globalHead,
            lastProcessed: 0,
          },
        });
      }
    }
  } catch (err) {
    send(ws, {
      type: 'error',
      message: err instanceof Error ? err.message : 'Failed to get projections',
    });
  }
}

// =============================================================================
// Subscription Management
// =============================================================================

function startSubscription(ws: BunWebSocket, ctx: AdminWsContext, state: ConnectionState, channel: Channel): void {
  if (state.subscriptions.has(channel)) {
    return;
  }

  state.subscriptions.add(channel);

  switch (channel) {
    case 'metrics':
      // Push immediately, then every 1 second
      void pushMetrics(ws, ctx);
      state.metricsInterval = setInterval(() => {
        void pushMetrics(ws, ctx);
      }, 1000);
      break;

    case 'logs':
      // Poll for new logs every 500ms
      state.logsInterval = setInterval(() => {
        void pushLogs(ws, ctx, state);
      }, 500);
      break;

    case 'events':
      // Push immediately, then poll every 500ms
      void pushEvents(ws, ctx, state);
      state.eventsInterval = setInterval(() => {
        void pushEvents(ws, ctx, state);
      }, 500);
      break;

    case 'projections':
      // Push immediately, then every 2 seconds
      void pushProjections(ws, ctx);
      state.projectionsInterval = setInterval(() => {
        void pushProjections(ws, ctx);
      }, 2000);
      break;
  }
}

function stopSubscription(state: ConnectionState, channel: Channel): void {
  if (!state.subscriptions.has(channel)) {
    return;
  }

  state.subscriptions.delete(channel);

  switch (channel) {
    case 'metrics':
      if (state.metricsInterval) {
        clearInterval(state.metricsInterval);
        state.metricsInterval = null;
      }
      break;

    case 'logs':
      if (state.logsInterval) {
        clearInterval(state.logsInterval);
        state.logsInterval = null;
      }
      break;

    case 'events':
      if (state.eventsInterval) {
        clearInterval(state.eventsInterval);
        state.eventsInterval = null;
      }
      break;

    case 'projections':
      if (state.projectionsInterval) {
        clearInterval(state.projectionsInterval);
        state.projectionsInterval = null;
      }
      break;
  }
}

// =============================================================================
// WebSocket Handler
// =============================================================================

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function createAdminWebSocketHandler(ctx: AdminWsContext): any {
  return {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    open(ws: any): void {
      const state = getState(ws);

      send(ws, { type: 'connected' });

      // Auto-subscribe to all channels by default
      const defaultChannels: Channel[] = ['metrics', 'logs', 'projections', 'events'];
      for (const channel of defaultChannels) {
        startSubscription(ws, ctx, state, channel);
      }

      send(ws, {
        type: 'subscribed',
        channels: defaultChannels,
      });
    },

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    message(ws: any, message: string | ArrayBuffer): void {
      const state = getState(ws);

      try {
        const msgStr = typeof message === 'string' ? message : new TextDecoder().decode(message);
        const data = JSON.parse(msgStr) as ClientMessage;

        if (data.type === 'subscribe' && Array.isArray(data.channels)) {
          const validChannels = data.channels.filter(
            (ch): ch is Channel => ['metrics', 'logs', 'projections', 'events'].includes(ch)
          );

          for (const channel of validChannels) {
            startSubscription(ws, ctx, state, channel);
          }

          send(ws, {
            type: 'subscribed',
            channels: validChannels,
          });
        } else if (data.type === 'unsubscribe' && Array.isArray(data.channels)) {
          const validChannels = data.channels.filter(
            (ch): ch is Channel => ['metrics', 'logs', 'projections', 'events'].includes(ch)
          );

          for (const channel of validChannels) {
            stopSubscription(state, channel);
          }

          send(ws, {
            type: 'unsubscribed',
            channels: validChannels,
          });
        }
      } catch {
        send(ws, {
          type: 'error',
          message: 'Invalid message format',
        });
      }
    },

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    close(ws: any): void {
      const state = connectionStates.get(ws);
      if (state) {
        clearIntervals(state);
        connectionStates.delete(ws);
      }
    },

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    drain(_ws: any): void {
      // Handle backpressure if needed
    },
  };
}
