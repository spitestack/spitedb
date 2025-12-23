/**
 * Todo Stats Projection
 *
 * Aggregated statistics per tenant.
 * Tracks total, completed, and active counts.
 */

import { defineProjection } from '@spitestack/db';

export default defineProjection(import.meta.path, {
  name: 'todo_stats',
  schema: {
    bucket: { type: 'text', primaryKey: true }, // 'total' for now
    total_count: 'integer',
    completed_count: 'integer',
    active_count: 'integer',
    last_updated: 'text',
  },
  getTenantId: (event) => event.tenantHash.toString(),
  apply(event, table) {
    const data = JSON.parse(event.data.toString());
    const stats = table['total'] ?? {
      total_count: 0,
      completed_count: 0,
      active_count: 0,
      last_updated: '',
    };

    switch (data.type) {
      case 'Created':
        stats.total_count++;
        stats.active_count++;
        break;
      case 'Completed':
        stats.completed_count++;
        stats.active_count = Math.max(0, stats.active_count - 1);
        break;
    }

    stats.last_updated = new Date(Number(event.timestampMs)).toISOString();
    table['total'] = stats;
  },
});
