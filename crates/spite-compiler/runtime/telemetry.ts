import type {
  MetricKindNapi,
  SpanStatusNapi,
  TelemetryDbNapi,
  TelemetryRecordNapi,
} from '@spitestack/db';

export type TelemetryAttrs = Record<string, unknown>;

export type TelemetrySpan = {
  traceId: string;
  spanId: string;
  parentSpanId?: string;
  name: string;
  tenantId: string;
  startMs: number;
  attrs?: TelemetryAttrs;
  commandId?: string;
};

const SEVERITY_INFO = 1;
const SEVERITY_WARN = 2;
const SEVERITY_ERROR = 3;

function attrsToJson(attrs?: TelemetryAttrs): string | undefined {
  if (!attrs) {
    return undefined;
  }
  const keys = Object.keys(attrs);
  if (keys.length === 0) {
    return undefined;
  }
  return JSON.stringify(attrs);
}

function mergeAttrs(base?: TelemetryAttrs, extra?: TelemetryAttrs): TelemetryAttrs | undefined {
  if (!base && !extra) {
    return undefined;
  }
  return { ...(base ?? {}), ...(extra ?? {}) };
}

export function startSpan(
  tenantId: string,
  traceId: string,
  name: string,
  parentSpanId?: string,
  attrs?: TelemetryAttrs,
  commandId?: string
): TelemetrySpan {
  return {
    traceId,
    spanId: crypto.randomUUID(),
    parentSpanId,
    name,
    tenantId,
    startMs: Date.now(),
    attrs,
    commandId,
  };
}

export function finishSpan(
  span: TelemetrySpan,
  status: SpanStatusNapi,
  endMs?: number,
  extraAttrs?: TelemetryAttrs
): TelemetryRecordNapi {
  const resolvedEnd = endMs ?? Date.now();
  const duration = Math.max(0, resolvedEnd - span.startMs);
  return {
    tsMs: span.startMs,
    kind: 'Span',
    tenantId: span.tenantId,
    traceId: span.traceId,
    spanId: span.spanId,
    parentSpanId: span.parentSpanId,
    name: span.name,
    spanStartMs: span.startMs,
    spanEndMs: resolvedEnd,
    spanDurationMs: duration,
    spanStatus: status,
    commandId: span.commandId,
    attrsJson: attrsToJson(mergeAttrs(span.attrs, extraAttrs)),
  };
}

export function metricCounter(
  tenantId: string,
  name: string,
  value: number,
  attrs?: TelemetryAttrs,
  traceId?: string,
  spanId?: string,
  commandId?: string
): TelemetryRecordNapi {
  return {
    tsMs: Date.now(),
    kind: 'Metric',
    tenantId,
    traceId,
    spanId,
    commandId,
    metricName: name,
    metricValue: value,
    metricKind: 'Counter' as MetricKindNapi,
    attrsJson: attrsToJson(attrs),
  };
}

export function metricHistogram(
  tenantId: string,
  name: string,
  value: number,
  attrs?: TelemetryAttrs,
  traceId?: string,
  spanId?: string,
  commandId?: string
): TelemetryRecordNapi {
  return {
    tsMs: Date.now(),
    kind: 'Metric',
    tenantId,
    traceId,
    spanId,
    commandId,
    metricName: name,
    metricValue: value,
    metricKind: 'Histogram' as MetricKindNapi,
    attrsJson: attrsToJson(attrs),
  };
}

export function logInfo(
  tenantId: string,
  message: string,
  attrs?: TelemetryAttrs,
  traceId?: string,
  spanId?: string,
  commandId?: string
): TelemetryRecordNapi {
  return {
    tsMs: Date.now(),
    kind: 'Log',
    tenantId,
    traceId,
    spanId,
    commandId,
    severity: SEVERITY_INFO,
    message,
    attrsJson: attrsToJson(attrs),
  };
}

export function logWarn(
  tenantId: string,
  message: string,
  attrs?: TelemetryAttrs,
  traceId?: string,
  spanId?: string,
  commandId?: string
): TelemetryRecordNapi {
  return {
    tsMs: Date.now(),
    kind: 'Log',
    tenantId,
    traceId,
    spanId,
    commandId,
    severity: SEVERITY_WARN,
    message,
    attrsJson: attrsToJson(attrs),
  };
}

export function logError(
  tenantId: string,
  message: string,
  attrs?: TelemetryAttrs,
  traceId?: string,
  spanId?: string,
  commandId?: string
): TelemetryRecordNapi {
  return {
    tsMs: Date.now(),
    kind: 'Log',
    tenantId,
    traceId,
    spanId,
    commandId,
    severity: SEVERITY_ERROR,
    message,
    attrsJson: attrsToJson(attrs),
  };
}

export function emitTelemetry(
  telemetry: TelemetryDbNapi,
  records: TelemetryRecordNapi[]
): void {
  if (records.length === 0) {
    return;
  }
  telemetry.writeBatch(records).catch(() => {
    // Telemetry should never take down the request path.
  });
}
