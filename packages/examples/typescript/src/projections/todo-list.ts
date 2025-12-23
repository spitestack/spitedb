/**
 * Todo List Projection
 *
 * Denormalized view of all todos.
 * Each todo is stored as a row with current state.
 */

import { defineProjection } from '@spitestack/db';

export default defineProjection(import.meta.path, {
  name: 'todo_list',
  schema: {
    id: { type: 'text', primaryKey: true },
    title: 'text',
    completed: 'integer', // SQLite boolean (0/1)
    completed_at: 'text',
    created_at: 'text',
  },
  // Use tenantHash directly - convert bigint to string for tenant ID
  getTenantId: (event) => event.tenantHash.toString(),
  apply(event, table) {
    const data = JSON.parse(event.data.toString());
    const id = event.streamId;

    switch (data.type) {
      case 'Created':
        table[id] = {
          title: data.title,
          completed: 0,
          completed_at: null,
          created_at: new Date(Number(event.timestampMs)).toISOString(),
        };
        break;
      case 'Completed': {
        const existing = table[id];
        if (existing) {
          existing.completed = 1;
          existing.completed_at = data.completedAt;
          table[id] = existing;
        }
        break;
      }
      case 'TitleUpdated': {
        const current = table[id];
        if (current) {
          current.title = data.title;
          table[id] = current;
        }
        break;
      }
    }
  },
});
