import App from "@spitestack/compiler/app";
import { TodoAggregate } from "./src/domain/aggregates/Todo/aggregate";

const app = App();

app.register(TodoAggregate, {
  scope: "public",
  methods: {
    rename: "auth",
  },
});

export default app;
