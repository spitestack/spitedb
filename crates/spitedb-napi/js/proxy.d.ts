/**
 * Magic Proxy for Projection Tables
 *
 * Provides a delightful DX for interacting with projection state:
 *
 * ```typescript
 * // Read a row
 * const user = table[userId];
 *
 * // Create/update a row
 * table[userId] = { loginCount: 0, totalSpent: 0 };
 *
 * // Increment a field (read + modify)
 * table[userId].loginCount++;
 *
 * // Delete a row
 * delete table[userId];
 * ```
 *
 * **Tenant Isolation**: All operations are scoped to the tenant_id passed
 * when creating the proxy. It's impossible to access another tenant's data.
 */
import type { ProjectionOp, RowData } from './types';
/** Native binding interface for projection operations */
export interface NativeBinding {
    /** Reads a row by tenant_id and key, returns JSON string or null */
    readProjectionRow(projectionName: string, tenantId: string, key: string): string | null;
}
/**
 * Creates a magic proxy for projection table access.
 *
 * The proxy intercepts property access to provide seamless read/write operations:
 * - `table[key]` reads from the database (synchronously, cached)
 * - `table[key] = value` queues an upsert operation
 * - `table[key].field++` reads, modifies, and queues an upsert
 * - `delete table[key]` queues a delete operation
 *
 * **Tenant Isolation**: All operations are scoped to the provided tenant_id.
 * It's impossible to access another tenant's data through this proxy.
 *
 * Call `flush()` to get all queued operations and reset the queue.
 */
export declare function createProjectionProxy<TRow extends RowData>(projectionName: string, tenantId: string, primaryKeyColumn: string, native: NativeBinding): {
    proxy: Record<string, TRow | undefined>;
    flush: () => ProjectionOp[];
};
//# sourceMappingURL=proxy.d.ts.map