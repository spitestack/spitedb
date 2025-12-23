/**
 * SpiteStack App Registration
 *
 * This module provides the App class for registering aggregates and orchestrators
 * with access configuration. The compiler reads these registrations to generate
 * appropriate authentication and authorization checks.
 *
 * @example
 * ```typescript
 * import { App } from '@spitestack/runtime/app';
 * import { OrderAggregate } from './domain/order';
 *
 * // Production mode enables schema locking
 * const app = new App({ mode: 'production' });
 *
 * app.register(OrderAggregate, {
 *   access: 'private',        // Default for all methods
 *   roles: ['user'],          // Default roles
 *   methods: {
 *     create: { access: 'public' },                    // No auth required
 *     cancel: { access: 'internal', roles: ['admin'] } // System-tenant role
 *   }
 * });
 *
 * app.start();
 * ```
 */

/**
 * Application mode controls schema evolution behavior.
 *
 * - `greenfield`: Development mode - schemas can change freely without constraints.
 *   Use this when building new features or during early development.
 *
 * - `production`: Locked mode - event schemas are captured in a lock file.
 *   Breaking changes are rejected, non-breaking changes auto-generate upcasts.
 *   Switch to this before deploying to production.
 */
export type AppMode = 'greenfield' | 'production';

/**
 * Top-level application configuration.
 */
export type AppConfig = {
  /**
   * Application mode controlling schema evolution.
   * @default 'greenfield'
   */
  mode?: AppMode;

  /**
   * Enable API versioning with contract locking.
   * When enabled, routes are prefixed with version (e.g., /v1/todo)
   * and breaking API changes require bumping to a new version.
   * @default false
   */
  apiVersioning?: boolean;
};

/**
 * Access levels for endpoints.
 *
 * - `public`: No authentication required - anyone can call this endpoint
 * - `internal`: Requires authentication + system-tenant membership (platform admin)
 * - `private`: Requires authentication and tenant membership
 */
export type AccessLevel = 'public' | 'internal' | 'private';

/**
 * Configuration for a single method.
 */
export type MethodConfig = {
  /** Access level for this method. Overrides entity-level default. */
  access?: AccessLevel;
  /** Required roles to access this method (only for internal/private). */
  roles?: string[];
};

/**
 * Configuration for an aggregate or orchestrator.
 *
 * @template T - The entity class type for method name autocompletion
 */
export type EntityConfig<T extends object = object> = {
  /** Default access level for all methods. Defaults to 'internal' if not specified. */
  access?: AccessLevel;
  /** Default required roles for all methods. */
  roles?: string[];
  /** Per-method configuration overrides. */
  methods?: {
    [K in keyof T]?: T[K] extends (...args: any[]) => any ? MethodConfig : never;
  };
};

/**
 * SpiteStack Application class for registering aggregates and orchestrators.
 *
 * The App class provides a FastAPI-style registration pattern for configuring
 * access control on your domain entities. The compiler reads these registrations
 * from your index.ts file to generate appropriate authentication and authorization
 * checks in the generated router.
 *
 * ## Access Levels
 *
 * | Level      | Auth Required | Tenant Required | System Tenant Required |
 * |------------|--------------|-----------------|----------------|
 * | `public`   | No           | No              | No                     |
 * | `internal` | Yes          | No              | Yes                    |
 * | `private`  | Yes          | Yes             | No                     |
 *
 * ## Default Behavior
 *
 * - If no App registration is found, all endpoints default to `internal`
 * - If an entity is registered without config, all methods default to `internal`
 * - Method-level config overrides entity-level config
 * - Roles on `public` endpoints are ignored (no auth = no role check)
 *
 * @example
 * ```typescript
 * const app = new App();
 *
 * // All methods internal (default)
 * app.register(AdminAggregate);
 *
 * // All methods private with user role
 * app.register(OrderAggregate, {
 *   access: 'private',
 *   roles: ['user']
 * });
 *
 * // Mixed access levels
 * app.register(ProductAggregate, {
 *   access: 'private',
 *   methods: {
 *     list: { access: 'public' },           // Anyone can list
 *     create: { access: 'internal' },       // Only system-tenant admins can create
 *     update: { roles: ['product_admin'] }  // Private + specific role
 *   }
 * });
 * ```
 */
export class App {
  private config: AppConfig;
  private registrations: Map<string, EntityConfig> = new Map();

  /**
   * Create a new SpiteStack application.
   *
   * @param config - Application configuration
   *
   * @example
   * ```typescript
   * // Development mode (default)
   * const app = new App();
   *
   * // Production mode with schema locking
   * const app = new App({ mode: 'production' });
   *
   * // Production with API versioning
   * const app = new App({ mode: 'production', apiVersioning: true });
   * ```
   */
  constructor(config: AppConfig = {}) {
    this.config = {
      mode: config.mode ?? 'greenfield',
      apiVersioning: config.apiVersioning ?? false,
    };
  }

  /**
   * Register an aggregate or orchestrator with access configuration.
   *
   * @param entity - The aggregate or orchestrator class
   * @param config - Optional access configuration
   * @returns this (for chaining)
   */
  register<T extends new (...args: any[]) => any>(
    entity: T,
    config?: EntityConfig<InstanceType<T>>
  ): this {
    this.registrations.set(entity.name, config ?? {});
    return this;
  }

  /**
   * Start the application server.
   *
   * Note: This method is a placeholder. The SpiteStack compiler generates
   * the actual server code based on the registrations. This method exists
   * for type-checking and documentation purposes.
   *
   * @param port - Optional port number (default: 3000)
   */
  async start(port: number = 3000): Promise<void> {
    // This is a stub - the compiler reads the registrations and generates
    // the actual server code. If this method is called directly, it means
    // the generated code wasn't used.
    throw new Error(
      'App.start() should not be called directly. ' +
      'Run `spitestack build` to generate the server, then run the generated code.'
    );
  }
}

export default App;
