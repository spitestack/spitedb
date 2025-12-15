/**
 * SpiteDB Projection System
 *
 * Provides a delightful API for building projections (read models) from event streams.
 *
 * @example
 * ```typescript
 * import { SpiteDB } from '@spitedb/napi';
 *
 * const userStats = projection('user_stats', {
 *   schema: {
 *     user_id: { type: 'text', primaryKey: true },
 *     login_count: { type: 'integer' },
 *     total_spent: { type: 'real' },
 *   },
 *
 *   async apply(event, table) {
 *     const data = JSON.parse(event.data.toString());
 *
 *     if (data.type === 'UserCreated') {
 *       table[event.streamId] = { login_count: 0, total_spent: 0 };
 *     }
 *
 *     if (data.type === 'UserLoggedIn') {
 *       table[event.streamId].login_count++;
 *     }
 *
 *     if (data.type === 'Purchase') {
 *       table[event.streamId].total_spent += data.amount;
 *     }
 *
 *     if (data.type === 'UserDeleted') {
 *       delete table[event.streamId];
 *     }
 *   }
 * });
 *
 * const db = await SpiteDB.open('events.db');
 * db.registerProjection(userStats);
 * await db.startProjections();
 * ```
 */
export { createProjectionProxy } from './proxy';
export { ProjectionRunner } from './runner';
/**
 * Creates a projection definition.
 *
 * A projection is a read model built from the event stream.
 * The `apply` function is called for each event and can modify
 * the projection table using a magic proxy syntax.
 *
 * @param name - Unique name for the projection (also the table name)
 * @param options - Projection configuration including schema and apply function
 * @returns A projection definition that can be registered with SpiteDB
 *
 * @example
 * ```typescript
 * const orderTotals = projection('order_totals', {
 *   schema: {
 *     order_id: { type: 'text', primaryKey: true },
 *     total: { type: 'real' },
 *     item_count: { type: 'integer' },
 *     status: { type: 'text' },
 *   },
 *
 *   apply(event, table) {
 *     const data = JSON.parse(event.data.toString());
 *
 *     switch (data.type) {
 *       case 'OrderCreated':
 *         table[data.orderId] = { total: 0, item_count: 0, status: 'pending' };
 *         break;
 *
 *       case 'ItemAdded':
 *         table[data.orderId].total += data.price;
 *         table[data.orderId].item_count++;
 *         break;
 *
 *       case 'OrderCompleted':
 *         table[data.orderId].status = 'completed';
 *         break;
 *     }
 *   }
 * });
 * ```
 */
export function projection(name, options) {
    return {
        name,
        options,
    };
}
