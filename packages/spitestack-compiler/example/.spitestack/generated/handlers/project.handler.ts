/**
 * Auto-generated handler for ProjectAggregate
 * DO NOT EDIT - regenerate with `spitestack compile`
 *
 * @generated from aggregates/Project/aggregate.ts
 */

import type { SpiteDbNapi } from "@spitestack/db";
import { ProjectAggregate } from "../../../src/domain/aggregates/Project/aggregate";
import type { ProjectEvent } from "../../../src/domain/aggregates/Project/events";
import {
  validateProjectCreate, validateProjectAddTodo, validateProjectRemoveTodo,
  type ValidationError as ValidationErrorType,
} from "../validators/project.validator";

/**
 * Error thrown when validation fails
 */
export class ValidationError extends Error {
  constructor(public readonly errors: ValidationErrorType[]) {
    super(`Validation failed: ${errors.map((e) => e.message).join(", ")}`);
    this.name = "ValidationError";
  }
}

export interface CommandResult {
  aggregateId: string;
  revision: number;
  events: unknown[];
}

export interface ProjectCommandContext {
  db: SpiteDbNapi;
  commandId: string;
  tenant: string;
  actorId?: string;
}

export interface ProjectCreateInput {
  id: string;
  name: string;
}

export interface ProjectAddTodoInput {
  id: string;
  todoId: string;
}

export interface ProjectRemoveTodoInput {
  id: string;
  todoId: string;
}

/**
 * Load aggregate, execute command, extract events, persist to SpiteDB
 */
type EventEnvelope<T> = {
  data: T;
  __meta: {
    tenantId: string;
    actorId?: string | null;
  };
};

function unwrapEvent<T>(event: T | EventEnvelope<T>): T {
  if (event && typeof event === "object" && "data" in event && "__meta" in event) {
    return (event as EventEnvelope<T>).data;
  }
  return event as T;
}

function wrapEvent<T>(event: T, tenantId: string, actorId?: string): EventEnvelope<T> {
  return {
    data: event,
    __meta: {
      tenantId,
      actorId: actorId ?? null,
    },
  };
}

async function executeCommand<TInput extends { id: string }>(
  ctx: ProjectCommandContext,
  input: TInput,
  execute: (aggregate: ProjectAggregate) => void
): Promise<CommandResult> {
  // Load existing events for this aggregate (fromRev=0, limit=10000)
  const existingEvents = await ctx.db.readStream(input.id, 0, 10000, ctx.tenant);

  // Create aggregate and replay events to reconstruct state
  const aggregate = new ProjectAggregate();

  for (const event of existingEvents) {
    const parsed = JSON.parse(event.data.toString()) as ProjectEvent | EventEnvelope<ProjectEvent>;
    aggregate.apply(unwrapEvent(parsed));
  }

  // Execute the command (populates aggregate.events)
  execute(aggregate);

  // Extract emitted events
  const newEvents = aggregate.events;

  if (newEvents.length === 0) {
    return {
      aggregateId: input.id,
      revision: existingEvents.length,
      events: [],
    };
  }

  // Persist to SpiteDB using optimized JSON path
  // expectedRev: 0 means stream must not exist, -1 means any revision
  const expectedRev = existingEvents.length === 0 ? 0 : existingEvents.length;
  const payload = JSON.stringify({
    streamId: input.id,
    commandId: ctx.commandId,
    expectedRev,
    events: newEvents.map((e) => wrapEvent(e, ctx.tenant, ctx.actorId)),
    tenant: ctx.tenant,
  });

  const result = await ctx.db.appendStreamJson(payload);

  return {
    aggregateId: input.id,
    revision: result.lastRev,
    events: newEvents,
  };
}

export const projectHandlers = {
  async create(ctx: ProjectCommandContext, input: ProjectCreateInput): Promise<CommandResult> {
    const result = validateProjectCreate(input);
    if (!result.success) {
      throw new ValidationError(result.errors);
    }
    const validated = result.data;

    return executeCommand(ctx, validated, (agg) => {
      agg.create(validated.name);
    });
  },

  async addTodo(ctx: ProjectCommandContext, input: ProjectAddTodoInput): Promise<CommandResult> {
    const result = validateProjectAddTodo(input);
    if (!result.success) {
      throw new ValidationError(result.errors);
    }
    const validated = result.data;

    return executeCommand(ctx, validated, (agg) => {
      agg.addTodo(validated.todoId);
    });
  },

  async removeTodo(ctx: ProjectCommandContext, input: ProjectRemoveTodoInput): Promise<CommandResult> {
    const result = validateProjectRemoveTodo(input);
    if (!result.success) {
      throw new ValidationError(result.errors);
    }
    const validated = result.data;

    return executeCommand(ctx, validated, (agg) => {
      agg.removeTodo(validated.todoId);
    });
  }
};

export type ProjectCommand =
  | { type: "project.create"; payload: ProjectCreateInput }
  | { type: "project.addTodo"; payload: ProjectAddTodoInput }
  | { type: "project.removeTodo"; payload: ProjectRemoveTodoInput };
