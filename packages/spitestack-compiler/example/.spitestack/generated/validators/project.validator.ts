/**
 * Auto-generated validators for ProjectAggregate
 * DO NOT EDIT - regenerate with `spitestack compile`
 *
 * @generated from aggregates/Project/aggregate.ts
 */

/**
 * Validation error details
 */
export interface ValidationError {
  path: string;
  message: string;
  expected: string;
  received: string;
}

/**
 * Result of validation - either success with typed data, or failure with errors
 */
export type ValidationResult<T> =
  | { success: true; data: T }
  | { success: false; errors: ValidationError[] };

/**
 * UUIDv7 regex pattern
 * Format: xxxxxxxx-xxxx-7xxx-yxxx-xxxxxxxxxxxx
 * Where y is 8, 9, a, or b
 */
const UUID_V7_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

/**
 * Fast UUIDv7 validation
 */
export function isUUIDv7(value: string): boolean {
  return UUID_V7_REGEX.test(value);
}


/**
 * Validate input for project.create
 */
export function validateProjectCreate(input: unknown): ValidationResult<{ id: string; name: string }> {
  const errors: ValidationError[] = [];

  if (typeof input !== "object" || input === null) {
    return { success: false, errors: [{ path: "", message: "Expected object", expected: "object", received: typeof input }] };
  }

  const obj = input as Record<string, unknown>;

  // Validate id (required UUIDv7)
  if (typeof obj.id !== "string") {
    errors.push({ path: "id", message: "Expected string", expected: "string", received: typeof obj.id });
  } else if (!isUUIDv7(obj.id)) {
    errors.push({ path: "id", message: "Expected UUIDv7", expected: "UUIDv7", received: obj.id });
  }

  // Validate name
  if (typeof obj.name !== "string") {
    errors.push({ path: "name", message: "Expected string", expected: "string", received: typeof obj.name });
  }

  if (errors.length > 0) {
    return { success: false, errors };
  }

  return { success: true, data: obj as { id: string; name: string } };
}

/**
 * Validate input for project.addTodo
 */
export function validateProjectAddTodo(input: unknown): ValidationResult<{ id: string; todoId: string }> {
  const errors: ValidationError[] = [];

  if (typeof input !== "object" || input === null) {
    return { success: false, errors: [{ path: "", message: "Expected object", expected: "object", received: typeof input }] };
  }

  const obj = input as Record<string, unknown>;

  // Validate id (required UUIDv7)
  if (typeof obj.id !== "string") {
    errors.push({ path: "id", message: "Expected string", expected: "string", received: typeof obj.id });
  } else if (!isUUIDv7(obj.id)) {
    errors.push({ path: "id", message: "Expected UUIDv7", expected: "UUIDv7", received: obj.id });
  }

  // Validate todoId
  if (typeof obj.todoId !== "string") {
    errors.push({ path: "todoId", message: "Expected string", expected: "string", received: typeof obj.todoId });
  }

  if (errors.length > 0) {
    return { success: false, errors };
  }

  return { success: true, data: obj as { id: string; todoId: string } };
}

/**
 * Validate input for project.removeTodo
 */
export function validateProjectRemoveTodo(input: unknown): ValidationResult<{ id: string; todoId: string }> {
  const errors: ValidationError[] = [];

  if (typeof input !== "object" || input === null) {
    return { success: false, errors: [{ path: "", message: "Expected object", expected: "object", received: typeof input }] };
  }

  const obj = input as Record<string, unknown>;

  // Validate id (required UUIDv7)
  if (typeof obj.id !== "string") {
    errors.push({ path: "id", message: "Expected string", expected: "string", received: typeof obj.id });
  } else if (!isUUIDv7(obj.id)) {
    errors.push({ path: "id", message: "Expected UUIDv7", expected: "UUIDv7", received: obj.id });
  }

  // Validate todoId
  if (typeof obj.todoId !== "string") {
    errors.push({ path: "todoId", message: "Expected string", expected: "string", received: typeof obj.todoId });
  }

  if (errors.length > 0) {
    return { success: false, errors };
  }

  return { success: true, data: obj as { id: string; todoId: string } };
}
