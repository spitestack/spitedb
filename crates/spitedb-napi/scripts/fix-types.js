#!/usr/bin/env node

/**
 * Fixes TypeScript declaration files after build:
 * 1. Appends JS exports to the NAPI-generated index.d.ts
 * 2. Adds node types reference to js/types.d.ts for Buffer type
 */

import { readFileSync, writeFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const rootDir = join(__dirname, '..');

// Fix index.d.ts - add JS exports
const indexDts = join(rootDir, 'index.d.ts');
let indexContent = readFileSync(indexDts, 'utf8');

const jsExports = `
// Re-export projection system types and functions
export type {
  ColumnDef,
  ColumnType,
  ErrorStrategy,
  Projection,
  ProjectionEvent,
  ProjectionOptions,
  ProjectionTable,
  RowData,
  SchemaDefinition,
  SchemaToRow,
} from './js/types';

export { projection, createProjectionProxy, ProjectionRunner } from './js/index';
`;

const telemetryTypes = `
export type TelemetryKindNapi = 'Log' | 'Metric' | 'Span'
export type MetricKindNapi = 'Gauge' | 'Counter' | 'Histogram' | 'Summary'
export type SpanStatusNapi = 'Unset' | 'Ok' | 'Error'
export type TelemetryOrderNapi = 'Asc' | 'Desc'
export type TimeSliceNapi = 'Daily'

export interface TelemetryConfigNapi {
  appName?: string
  partitions?: number
  batchMaxMs?: number
  batchMaxBytes?: number
  batchMaxRecords?: number
  maxInflight?: number
  retentionDays?: number
  timeSlice?: TimeSliceNapi
  defaultService?: string
}

export interface TelemetryCursorNapi {
  slice: string
  lastIds: Array<number>
}

export interface TelemetryRecordNapi {
  tsMs: number
  kind: TelemetryKindNapi
  tenantId?: string
  tenantHash?: number
  eventGlobalPos?: number
  streamId?: string
  streamHash?: number
  streamRev?: number
  commandId?: string
  traceId?: string
  spanId?: string
  parentSpanId?: string
  name?: string
  service?: string
  severity?: number
  message?: string
  metricName?: string
  metricValue?: number
  metricKind?: MetricKindNapi
  metricUnit?: string
  spanStartMs?: number
  spanEndMs?: number
  spanDurationMs?: number
  spanStatus?: SpanStatusNapi
  attrsJson?: string
}

export interface TelemetryQueryNapi {
  tenantId?: string
  tenantHash?: number
  kind?: TelemetryKindNapi
  startMs?: number
  endMs?: number
  severity?: number
  metricName?: string
  eventGlobalPos?: number
  streamId?: string
  streamHash?: number
  streamRev?: number
  commandId?: string
  traceId?: string
  limit?: number
  order?: TelemetryOrderNapi
  slice?: string
  shardId?: number
}

export interface TelemetryTailResultNapi {
  records: Array<TelemetryRecordNapi>
  cursor: TelemetryCursorNapi
}

export declare class TelemetryDbNapi {
  static open(root: string, config?: TelemetryConfigNapi): Promise<TelemetryDbNapi>
  write(record: TelemetryRecordNapi): Promise<void>
  writeBatch(records: Array<TelemetryRecordNapi>): Promise<void>
  flush(): Promise<void>
  query(query: TelemetryQueryNapi): Promise<Array<TelemetryRecordNapi>>
  tail(cursor: TelemetryCursorNapi, limit: number): Promise<TelemetryTailResultNapi>
  cleanupRetention(): Promise<void>
}
`;

if (!indexContent.includes('Re-export projection system')) {
  indexContent += jsExports;
  writeFileSync(indexDts, indexContent);
  console.log('Added JS exports to index.d.ts');
} else {
  console.log('JS exports already present in index.d.ts');
}

if (!indexContent.includes('TelemetryDbNapi')) {
  indexContent += telemetryTypes;
  writeFileSync(indexDts, indexContent);
  console.log('Added telemetry types to index.d.ts');
}

// Ensure DEFAULT_TENANT export is present
const defaultTenantExport = '\n/** Default tenant ID for single-tenant apps. */\nexport const DEFAULT_TENANT: string\n';
if (!indexContent.includes('DEFAULT_TENANT')) {
  indexContent += defaultTenantExport;
  writeFileSync(indexDts, indexContent);
  console.log('Added DEFAULT_TENANT to index.d.ts');
} else {
  console.log('DEFAULT_TENANT already present in index.d.ts');
}

// Ensure SpiteDBNapi value export is present
const spiteDbValueExport = '\nexport const SpiteDBNapi: typeof SpiteDbNapi\n';
if (!indexContent.includes('export const SpiteDBNapi')) {
  indexContent += spiteDbValueExport;
  writeFileSync(indexDts, indexContent);
  console.log('Added SpiteDBNapi value export to index.d.ts');
} else {
  console.log('SpiteDBNapi value export already present in index.d.ts');
}

// Ensure TelemetryDbNapi value export is present
const telemetryValueExport = '\nexport const TelemetryDBNapi: typeof TelemetryDbNapi\n';
if (indexContent.includes('TelemetryDbNapi') && !indexContent.includes('TelemetryDBNapi')) {
  indexContent += telemetryValueExport;
  writeFileSync(indexDts, indexContent);
  console.log('Added TelemetryDBNapi value export to index.d.ts');
}

// Fix js/types.d.ts - add node types reference for Buffer
const typesDts = join(rootDir, 'js', 'types.d.ts');
let typesContent = readFileSync(typesDts, 'utf8');

const nodeRef = '/// <reference types="node" />\n';
if (!typesContent.includes('<reference types="node"')) {
  typesContent = nodeRef + typesContent;
  writeFileSync(typesDts, typesContent);
  console.log('Added node types reference to js/types.d.ts');
} else {
  console.log('Node types reference already present in js/types.d.ts');
}

// Ensure DEFAULT_TENANT export exists in runtime index.js
const indexJs = join(rootDir, 'index.js');
let indexJsContent = readFileSync(indexJs, 'utf8');
if (indexJsContent.includes('const { SpiteDbNapi } = nativeBinding') && !indexJsContent.includes('TelemetryDbNapi')) {
  indexJsContent = indexJsContent.replace(
    'const { SpiteDbNapi } = nativeBinding',
    'const { SpiteDbNapi, TelemetryDbNapi } = nativeBinding'
  );
  writeFileSync(indexJs, indexJsContent);
  console.log('Updated native binding destructure to include TelemetryDbNapi');
}

const runtimeSpiteDbAlias = "\nmodule.exports.SpiteDBNapi = SpiteDbNapi\n";
const runtimeTelemetryAlias = "\nmodule.exports.TelemetryDbNapi = TelemetryDbNapi\nmodule.exports.TelemetryDBNapi = TelemetryDbNapi\n";
const runtimeDefaultTenant = "\n// Default tenant for single-tenant apps\nmodule.exports.DEFAULT_TENANT = 'default'\n";
if (!indexJsContent.includes('SpiteDBNapi')) {
  indexJsContent += runtimeSpiteDbAlias;
  writeFileSync(indexJs, indexJsContent);
  console.log('Added SpiteDBNapi alias to index.js');
} else {
  console.log('SpiteDBNapi alias already present in index.js');
}
if (indexJsContent.includes('TelemetryDbNapi') && !indexJsContent.includes('TelemetryDBNapi')) {
  indexJsContent += runtimeTelemetryAlias;
  writeFileSync(indexJs, indexJsContent);
  console.log('Added TelemetryDbNapi alias to index.js');
}
if (!indexJsContent.includes('DEFAULT_TENANT')) {
  indexJsContent += runtimeDefaultTenant;
  writeFileSync(indexJs, indexJsContent);
  console.log('Added DEFAULT_TENANT to index.js');
} else {
  console.log('DEFAULT_TENANT already present in index.js');
}
