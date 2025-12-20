/**
 * Project Domain Events
 */

export type ProjectEvent =
  | { type: "ProjectCreated"; name: string }
  | { type: "TodoAddedToProject"; todoId: string }
  | { type: "TodoRemovedFromProject"; todoId: string };
