/**
 * TransferTodoBetweenProjects Orchestrator
 *
 * Moves a todo from one project to another atomically.
 * Demonstrates:
 * - Object-style parameters in orchestrate()
 * - Multiple aggregates of the same type (source and target projects)
 * - Complex business logic spanning multiple aggregates
 */

import { TodoAggregate } from "../../aggregates/Todo/aggregate";
import { ProjectAggregate } from "../../aggregates/Project/aggregate";

export class TransferTodoBetweenProjectsOrchestrator {
  constructor(
    private todo: TodoAggregate,
    private sourceProject: ProjectAggregate,
    private targetProject: ProjectAggregate
  ) {}

  /**
   * Transfer the todo from source project to target project.
   *
   * @param input.validateCompletion - If true, prevents transferring completed todos
   */
  async orchestrate(input: { validateCompletion?: boolean }): Promise<void> {
    const todoState = this.todo.currentState;
    const sourceState = this.sourceProject.currentState;
    const targetState = this.targetProject.currentState;

    // Validate todo exists
    if (!todoState.title) {
      throw new Error("Todo does not exist");
    }

    // Optional completion check
    if (input.validateCompletion && todoState.completed) {
      throw new Error("Cannot transfer completed todo");
    }

    // Validate projects exist
    if (!sourceState.name) {
      throw new Error("Source project does not exist");
    }
    if (!targetState.name) {
      throw new Error("Target project does not exist");
    }

    // Validate todo is in source project
    const todoId = todoState.title; // Using title as ID for demo
    if (!sourceState.todoIds.includes(todoId)) {
      throw new Error("Todo is not in source project");
    }

    // Validate todo is not already in target project
    if (targetState.todoIds.includes(todoId)) {
      throw new Error("Todo is already in target project");
    }

    // Atomically remove from source and add to target
    this.sourceProject.removeTodo(todoId);
    this.targetProject.addTodo(todoId);

    // Both project aggregates will have their events committed atomically
  }
}
