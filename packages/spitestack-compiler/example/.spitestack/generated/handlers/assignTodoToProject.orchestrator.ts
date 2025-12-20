/**
 * Auto-generated handler for AssignTodoToProjectOrchestrator
 * DO NOT EDIT - regenerate with `spitestack compile`
 *
 * @generated from orchestrators/AssignTodoToProject/orchestrator.ts
 */

import type { SpiteDbNapi } from "@spitestack/db";
import { AssignTodoToProjectOrchestrator } from "../../../src/domain/orchestrators/AssignTodoToProject/orchestrator";
import { TodoAggregate } from "../../../src/domain/aggregates/Todo/aggregate";
import { ProjectAggregate } from "../../../src/domain/aggregates/Project/aggregate";

export interface OrchestratorHandlerContext {
  db: SpiteDbNapi;
  commandId: string;
  tenant: string;
  actorId?: string;
  adapters: Record<string, unknown>;
}

export interface AssignTodoToProjectInput {
  todoId: string;
  projectId: string;
}

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

interface AggregateBase {
  events: unknown[];
  apply(event: unknown): void;
}

interface LoadedAggregate<T extends AggregateBase> {
  aggregate: T;
  id: string;
  revision: number;
}

async function loadAggregate<T extends AggregateBase>(
  db: SpiteDbNapi,
  AggregateClass: new () => T,
  aggregateId: string,
  tenant: string
): Promise<LoadedAggregate<T>> {
  const existingEvents = await db.readStream(aggregateId, 0, 10000, tenant);

  const aggregate = new AggregateClass();

  for (const event of existingEvents) {
    const parsed = JSON.parse(event.data.toString());
    aggregate.apply(unwrapEvent(parsed));
  }

  return {
    aggregate,
    id: aggregateId,
    revision: existingEvents.length,
  };
}

interface AtomicCommitResult {
  eventCount: number;
  revisions: Map<string, number>;
}

async function commitAggregatesAtomic(
  db: SpiteDbNapi,
  aggregates: Array<{ aggregate: AggregateBase; id: string; revision: number }>,
  commandId: string,
  tenant: string,
  actorId?: string
): Promise<AtomicCommitResult> {
  const commands: Array<{
    streamId: string;
    commandId: string;
    expectedRev: number;
    events: unknown[];
  }> = [];

  let totalEvents = 0;

  for (const { aggregate, id, revision } of aggregates) {
    const newEvents = aggregate.events;

    if (newEvents.length === 0) {
      continue;
    }

    totalEvents += newEvents.length;

    commands.push({
      streamId: id,
      commandId: `${commandId}:${id}`,
      expectedRev: revision === 0 ? 0 : revision,
      events: newEvents.map((e) => wrapEvent(e, tenant, actorId)),
    });
  }

  if (commands.length === 0) {
    return { eventCount: 0, revisions: new Map() };
  }

  // Use appendBatchJson for atomic multi-stream commit with optimized JSON path
  const payload = JSON.stringify({ commands, tenant });
  const results = await db.appendBatchJson(payload);

  const revisions = new Map<string, number>();
  for (let i = 0; i < commands.length; i++) {
    revisions.set(commands[i].streamId, results[i].lastRev);
  }

  return { eventCount: totalEvents, revisions };
}

export async function assignTodoToProjectHandler(
  ctx: OrchestratorHandlerContext,
  input: AssignTodoToProjectInput
): Promise<void> {
  // 1. Load aggregates
  const [todoLoaded, projectLoaded] = await Promise.all([
    loadAggregate(ctx.db, TodoAggregate, input.todoId, ctx.tenant),
    loadAggregate(ctx.db, ProjectAggregate, input.projectId, ctx.tenant)
  ]);

  // 2. Get adapters
  const notifications = ctx.adapters["notification"];

  // 3. Instantiate orchestrator with dependencies
  const orchestrator = new AssignTodoToProjectOrchestrator(todoLoaded.aggregate, projectLoaded.aggregate, notifications);

  // 4. Execute orchestration
  await orchestrator.orchestrate();

  // 5. Atomic commit of all aggregate events
  await commitAggregatesAtomic(
    ctx.db,
    [todoLoaded, projectLoaded],
    ctx.commandId,
    ctx.tenant,
    ctx.actorId
  );
}
