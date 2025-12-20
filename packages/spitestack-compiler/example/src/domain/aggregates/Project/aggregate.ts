/**
 * Project Aggregate
 */

import type { ProjectEvent } from "./events";

export type ProjectState = {
  name: string;
  todoIds: string[];
};

export class ProjectAggregate {
  static readonly initialState: ProjectState = {
    name: "",
    todoIds: [],
  };

  readonly events: ProjectEvent[] = [];
  private state: ProjectState;

  constructor(initialState: ProjectState = ProjectAggregate.initialState) {
    this.state = { ...initialState, todoIds: [...initialState.todoIds] };
  }

  get currentState(): ProjectState {
    return this.state;
  }

  protected emit(event: ProjectEvent): void {
    this.events.push(event);
    this.apply(event);
  }

  apply(event: ProjectEvent): void {
    switch (event.type) {
      case "ProjectCreated":
        this.state.name = event.name;
        break;
      case "TodoAddedToProject":
        if (!this.state.todoIds.includes(event.todoId)) {
          this.state.todoIds.push(event.todoId);
        }
        break;
      case "TodoRemovedFromProject":
        this.state.todoIds = this.state.todoIds.filter((id) => id !== event.todoId);
        break;
    }
  }

  // Commands
  create(name: string): void {
    if (this.state.name) {
      throw new Error("Project already exists");
    }
    this.emit({ type: "ProjectCreated", name });
  }

  addTodo(todoId: string): void {
    if (!this.state.name) {
      throw new Error("Project does not exist");
    }
    if (this.state.todoIds.includes(todoId)) {
      throw new Error("Todo already in project");
    }
    this.emit({ type: "TodoAddedToProject", todoId });
  }

  removeTodo(todoId: string): void {
    if (!this.state.name) {
      throw new Error("Project does not exist");
    }
    if (!this.state.todoIds.includes(todoId)) {
      throw new Error("Todo not in project");
    }
    this.emit({ type: "TodoRemovedFromProject", todoId });
  }
}
