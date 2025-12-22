import { TodoEvent } from "./events";
import { TodoState } from "./state";

export class TodoAggregate {
  static readonly initialState: TodoState = {
    id: "",
    title: "",
    completed: false,
    completedAt: undefined,
  };

  readonly events: TodoEvent[] = [];
  private state: TodoState;

  constructor(initialState: TodoState = TodoAggregate.initialState) {
    this.state = { ...initialState };
  }

  get currentState(): TodoState {
    return this.state;
  }

  protected emit(event: TodoEvent): void {
    this.events.push(event);
    this.apply(event);
  }

  apply(event: TodoEvent): void {
    switch (event.type) {
      case "Created":
        this.state.id = event.id;
        this.state.title = event.title;
        break;
      case "Completed":
        this.state.completed = true;
        this.state.completedAt = event.completedAt;
        break;
      case "TitleUpdated":
        this.state.title = event.title;
        break;
    }
  }

  // Commands
  create(id: string, title: string): void {
    if (!title) {
      throw new Error("Title is required");
    }
    this.emit({ type: "Created", id, title });
  }

  complete(): void {
    if (this.state.completed) {
      throw new Error("Already completed");
    }
    this.emit({ type: "Completed", completedAt: new Date().toISOString() });
  }

  updateTitle(title: string): void {
    if (!title) {
      throw new Error("Title is required");
    }
    this.emit({ type: "TitleUpdated", title });
  }
}
