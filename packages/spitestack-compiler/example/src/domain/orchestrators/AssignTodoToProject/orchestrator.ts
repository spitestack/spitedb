/**
 * AssignTodoToProject Orchestrator
 *
 * Coordinates assigning a todo to a project atomically.
 * Both aggregates are updated in a single transaction.
 */

import { TodoAggregate } from "../../aggregates/Todo/aggregate";
import { ProjectAggregate } from "../../aggregates/Project/aggregate";

/**
 * Optional adapter for sending notifications.
 * Demonstrates how external services integrate with orchestrators.
 */
export interface NotificationAdapter {
  sendAssignmentNotification(todoTitle: string, projectName: string): Promise<void>;
}

export class AssignTodoToProjectOrchestrator {
  constructor(
    private todo: TodoAggregate,
    private project: ProjectAggregate,
    private notifications?: NotificationAdapter
  ) {}

  /**
   * Assign the todo to the project.
   *
   * This orchestrator:
   * 1. Validates the todo exists and is not completed
   * 2. Validates the project exists
   * 3. Adds the todo to the project
   * 4. Optionally sends a notification
   *
   * All aggregate changes are committed atomically via appendBatch.
   */
  async orchestrate(): Promise<void> {
    // Validate todo state
    const todoState = this.todo.currentState;
    if (!todoState.title) {
      throw new Error("Todo does not exist");
    }
    if (todoState.completed) {
      throw new Error("Cannot assign completed todo to project");
    }

    // Validate project state
    const projectState = this.project.currentState;
    if (!projectState.name) {
      throw new Error("Project does not exist");
    }

    // Add todo to project (this emits an event on the project aggregate)
    this.project.addTodo(this.todo.currentState.title); // Using title as ID for demo

    // Optional: send notification via adapter
    if (this.notifications) {
      await this.notifications.sendAssignmentNotification(
        todoState.title,
        projectState.name
      );
    }

    // Events from both aggregates will be committed atomically
    // by the generated handler using appendBatch
  }
}
