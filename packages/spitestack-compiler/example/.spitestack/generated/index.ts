/**
 * Auto-generated SpiteStack exports
 * DO NOT EDIT - regenerate with `spitestack compile`
 */

// Wiring
export { executeCommand, type Command, type CommandContext, type CommandResult } from "./wiring";
export { executeOrchestrator, type OrchestratorContext, type OrchestratorName, type OrchestratorInput } from "./wiring";

// Handlers
export { todoHandlers, type TodoCommand } from "./handlers/todo.handler";
export { projectHandlers, type ProjectCommand } from "./handlers/project.handler";

// Orchestrators
export { assignTodoToProjectHandler, type AssignTodoToProjectInput } from "./handlers/assignTodoToProject.orchestrator";
export { transferTodoBetweenProjectsHandler, type TransferTodoBetweenProjectsInput } from "./handlers/transferTodoBetweenProjects.orchestrator";

// Validators
export { validateTodoCreate, validateTodoComplete, validateTodoRename } from "./validators/todo.validator";
export { validateProjectCreate, validateProjectAddTodo, validateProjectRemoveTodo } from "./validators/project.validator";

// Auth
export { createSpiteStackApp, createSpiteStackAuth } from "./auth";

// Routes
export { createCommandHandler } from "./routes";
