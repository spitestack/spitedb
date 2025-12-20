/**
 * Auto-generated SpiteDB wiring
 * DO NOT EDIT - regenerate with `spitestack compile`
 */

import type { SpiteDbNapi } from "@spitestack/db";
import { todoHandlers, type TodoCommand } from "./handlers/todo.handler";
import { projectHandlers, type ProjectCommand } from "./handlers/project.handler";

import { assignTodoToProjectHandler, type AssignTodoToProjectInput } from "./handlers/assignTodoToProject.orchestrator";
import { transferTodoBetweenProjectsHandler, type TransferTodoBetweenProjectsInput } from "./handlers/transferTodoBetweenProjects.orchestrator";

/**
 * Union of all command types
 */
export type Command = Extract<TodoCommand, { type: "todo.create" | "todo.complete" | "todo.rename" }> | Extract<ProjectCommand, { type: "project.create" | "project.addTodo" | "project.removeTodo" }>;

/**
 * Context required for command execution
 */
export interface CommandContext {
  db: SpiteDbNapi;
  commandId: string;
  tenant: string;
  actorId?: string;
}

/**
 * Context required for orchestrator execution
 */
export interface OrchestratorContext {
  db: SpiteDbNapi;
  commandId: string;
  tenant: string;
  actorId?: string;
  adapters: Record<string, unknown>;
}

/**
 * Result of command execution
 */
export interface CommandResult {
  aggregateId: string;
  revision: number;
  events: unknown[];
}

/**
 * Execute a command and persist events to SpiteDB
 */
export async function executeCommand(
  ctx: CommandContext,
  command: Command
): Promise<CommandResult> {
  switch (command.type) {
    case "todo.create":
      return todoHandlers.create(ctx, command.payload);

    case "todo.complete":
      return todoHandlers.complete(ctx, command.payload);

    case "todo.rename":
      return todoHandlers.rename(ctx, command.payload);

    case "project.create":
      return projectHandlers.create(ctx, command.payload);

    case "project.addTodo":
      return projectHandlers.addTodo(ctx, command.payload);

    case "project.removeTodo":
      return projectHandlers.removeTodo(ctx, command.payload);

    default:
      const _exhaustive: never = command;
      throw new Error(`Unknown command type: ${(command as any).type}`);
  }
}

/**
 * Union of all orchestrator input types
 */
export type OrchestratorInput = AssignTodoToProjectInput | TransferTodoBetweenProjectsInput;

/**
 * Available orchestrator names
 */
export type OrchestratorName = "assignTodoToProject" | "transferTodoBetweenProjects";

/**
 * Execute an orchestrator
 */
export async function executeOrchestrator(
  ctx: OrchestratorContext,
  name: OrchestratorName,
  input: OrchestratorInput
): Promise<void> {
  switch (name) {
    case "assignTodoToProject":
      return assignTodoToProjectHandler(ctx, input as AssignTodoToProjectInput);

    case "transferTodoBetweenProjects":
      return transferTodoBetweenProjectsHandler(ctx, input as TransferTodoBetweenProjectsInput);

    default:
      const _exhaustive: never = name;
      throw new Error(`Unknown orchestrator: ${name}`);
  }
}

// Re-export handlers for direct access
export { todoHandlers } from "./handlers/todo.handler";
export { projectHandlers } from "./handlers/project.handler";

// Re-export orchestrator handlers
export { assignTodoToProjectHandler } from "./handlers/assignTodoToProject.orchestrator";
export { transferTodoBetweenProjectsHandler } from "./handlers/transferTodoBetweenProjects.orchestrator";
