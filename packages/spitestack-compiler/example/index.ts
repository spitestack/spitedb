import App from "@spitestack/compiler/app";
import { TodoAggregate } from "./src/domain/aggregates/Todo/aggregate";
import { ProjectAggregate } from "./src/domain/aggregates/Project/aggregate";

const app = App();

// Register aggregates
app.register(TodoAggregate, {
  scope: "public",
  methods: {
    rename: "auth",
  },
});

app.register(ProjectAggregate, {
  scope: "auth",
});

// Orchestrators are auto-discovered from domain/orchestrators/**\/orchestrator.ts
// They can also be explicitly registered:
// app.orchestrator({
//   orchestrator: AssignTodoToProjectOrchestrator,
//   scope: "auth",
//   route: "/todos/assign-to-project",
// });

export default app;
