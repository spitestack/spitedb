import { describe, it, expect } from "bun:test";
import {
  validateTodoCreateInput,
  validateTodoCompleteInput,
  validateTodoUpdateTitleInput,
} from "../.spitestack/src/generated/validators/todo.validator";

describe("Generated Validators", () => {
  describe("validateTodoCreateInput", () => {
    it("should accept valid input", () => {
      const result = validateTodoCreateInput({ id: "123", title: "Buy milk" });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value.id).toBe("123");
        expect(result.value.title).toBe("Buy milk");
      }
    });

    it("should reject non-object input", () => {
      const result = validateTodoCreateInput("not an object");
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.errors[0].field).toBe("_root");
      }
    });

    it("should reject missing id", () => {
      const result = validateTodoCreateInput({ title: "Buy milk" });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.errors.some((e) => e.field === "id")).toBe(true);
      }
    });

    it("should reject missing title", () => {
      const result = validateTodoCreateInput({ id: "123" });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.errors.some((e) => e.field === "title")).toBe(true);
      }
    });

    it("should reject wrong type for id", () => {
      const result = validateTodoCreateInput({ id: 123, title: "Buy milk" });
      expect(result.ok).toBe(false);
      if (!result.ok) {
        expect(result.errors.some((e) => e.field === "id")).toBe(true);
      }
    });
  });

  describe("validateTodoCompleteInput", () => {
    it("should accept empty object", () => {
      const result = validateTodoCompleteInput({});
      expect(result.ok).toBe(true);
    });

    it("should reject non-object input", () => {
      const result = validateTodoCompleteInput(null);
      expect(result.ok).toBe(false);
    });
  });

  describe("validateTodoUpdateTitleInput", () => {
    it("should accept valid input", () => {
      const result = validateTodoUpdateTitleInput({ title: "New title" });
      expect(result.ok).toBe(true);
      if (result.ok) {
        expect(result.value.title).toBe("New title");
      }
    });

    it("should reject missing title", () => {
      const result = validateTodoUpdateTitleInput({});
      expect(result.ok).toBe(false);
    });
  });
});
