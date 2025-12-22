import { describe, it, expect } from "bun:test";
import { TodoAggregate } from "../src/domain/Todo/aggregate";

describe("TodoAggregate", () => {
  it("should create a todo", () => {
    const agg = new TodoAggregate();
    agg.create("1", "Buy milk");

    expect(agg.currentState.id).toBe("1");
    expect(agg.currentState.title).toBe("Buy milk");
    expect(agg.currentState.completed).toBe(false);
    expect(agg.events).toHaveLength(1);
    expect(agg.events[0]).toEqual({ type: "Created", id: "1", title: "Buy milk" });
  });

  it("should reject empty title on create", () => {
    const agg = new TodoAggregate();
    expect(() => agg.create("1", "")).toThrow("Title is required");
  });

  it("should complete a todo", () => {
    const agg = new TodoAggregate();
    agg.create("1", "Buy milk");
    agg.complete();

    expect(agg.currentState.completed).toBe(true);
    expect(agg.currentState.completedAt).toBeDefined();
    expect(agg.events).toHaveLength(2);
  });

  it("should reject completing an already completed todo", () => {
    const agg = new TodoAggregate();
    agg.create("1", "Buy milk");
    agg.complete();

    expect(() => agg.complete()).toThrow("Already completed");
  });

  it("should update title", () => {
    const agg = new TodoAggregate();
    agg.create("1", "Buy milk");
    agg.updateTitle("Buy oat milk");

    expect(agg.currentState.title).toBe("Buy oat milk");
    expect(agg.events).toHaveLength(2);
  });

  it("should reject empty title on update", () => {
    const agg = new TodoAggregate();
    agg.create("1", "Buy milk");

    expect(() => agg.updateTitle("")).toThrow("Title is required");
  });

  it("should replay events to reconstruct state", () => {
    // First aggregate creates some events
    const agg1 = new TodoAggregate();
    agg1.create("1", "Buy milk");
    agg1.complete();

    // Second aggregate reconstructs state from events
    const agg2 = new TodoAggregate();
    for (const event of agg1.events) {
      agg2.apply(event);
    }

    expect(agg2.currentState).toEqual(agg1.currentState);
  });
});
