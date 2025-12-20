import type ts from "typescript";

// ============================================================================
// Configuration Types
// ============================================================================

/**
 * Mode determines how schema evolution and API contracts are handled.
 * - "greenfield": No lock files, schemas can change freely. Use during early development.
 * - "production": Lock files enforced. Breaking changes require upcasters/version bumps.
 */
export type SpiteStackMode = "greenfield" | "production";

export interface CompilerConfig {
  /**
   * Mode determines schema evolution behavior.
   * - "greenfield": No lock files, iterate freely (default)
   * - "production": Lock files enforced, breaking changes require upcasters
   */
  mode: SpiteStackMode;
  /** Directory containing aggregate classes */
  domainDir: string;
  /** Output directory for generated code */
  outDir: string;
  /** Glob patterns to include */
  include: string[];
  /** Glob patterns to exclude */
  exclude: string[];
  /** Code generation options */
  generate: {
    handlers: boolean;
    validators: boolean;
    wiring: boolean;
  };
  /** Error reporting options */
  diagnostics: {
    colors: boolean;
    maxErrors: number;
  };
  routes: {
    basePath: string;
    publicSessionHeader: string;
    publicSessionRequired: boolean;
    publicTenantId?: string;
  };
  /** Schema evolution options (relevant in production mode) */
  schemaEvolution: {
    /** Directory for upcaster files */
    upcasterDir: string;
  };
  /** API versioning options (opt-in for API products) */
  api: {
    /** Enable API contract versioning (default: false) */
    versioning: boolean;
    /** Optional alias for latest version (e.g., "/api") */
    latestAlias?: string;
    /** Warn when deprecated API versions are used */
    deprecationWarnings: boolean;
  };
  /** App config path if using spitestack.app */
  appPath?: string | null;
  /** Raw app config */
  appConfig?: SpiteStackAppConfig | null;
  /** App registrations when using App().register */
  registrations?: SpiteStackRegistration[] | null;
}

export type PartialConfig = Partial<Omit<CompilerConfig, "generate" | "diagnostics" | "schemaEvolution" | "api">> & {
  generate?: Partial<CompilerConfig["generate"]>;
  diagnostics?: Partial<CompilerConfig["diagnostics"]>;
  schemaEvolution?: Partial<CompilerConfig["schemaEvolution"]>;
  api?: Partial<CompilerConfig["api"]>;
};

export interface SpiteStackAppConfig {
  mode?: SpiteStackMode;
  domainDir?: string;
  outDir?: string;
  include?: string[];
  exclude?: string[];
  generate?: {
    handlers?: boolean;
    validators?: boolean;
    wiring?: boolean;
  };
  diagnostics?: {
    colors?: boolean;
    maxErrors?: number;
  };
  routes?: {
    basePath?: string;
    publicSessionHeader?: string;
    publicSessionRequired?: boolean;
    publicTenantId?: string;
  };
  schemaEvolution?: {
    upcasterDir?: string;
  };
  api?: {
    versioning?: boolean;
    latestAlias?: string;
    deprecationWarnings?: boolean;
  };
  auth?: unknown;
}

export type SpiteStackScope = "public" | "auth" | "internal";

export type SpiteStackMethodScope =
  | SpiteStackScope
  | false
  | {
      scope?: SpiteStackScope;
      roles?: string[];
    };

export interface SpiteStackRegistration {
  aggregate: string;
  scope?: SpiteStackScope;
  roles?: string[];
  methods?: Record<string, SpiteStackMethodScope>;
}

// ============================================================================
// Analysis Types
// ============================================================================

export interface DiscoveredFile {
  path: string;
  relativePath: string;
}

export interface ParsedFile {
  filePath: string;
  sourceFile: ts.SourceFile;
  classes: ts.ClassDeclaration[];
  typeAliases: ts.TypeAliasDeclaration[];
}

export interface TypeInfo {
  kind:
    | "string"
    | "number"
    | "boolean"
    | "null"
    | "undefined"
    | "object"
    | "array"
    | "union"
    | "literal"
    | "unknown";
  properties?: Record<string, TypeInfo>;
  elementType?: TypeInfo;
  types?: TypeInfo[];
  literalValue?: string | number | boolean;
}

export interface ParameterInfo {
  name: string;
  type: TypeInfo;
  optional: boolean;
  node: ts.ParameterDeclaration;
}

export interface CommandInfo {
  methodName: string;
  parameters: ParameterInfo[];
  node: ts.MethodDeclaration;
}

export interface EventVariant {
  type: string; // discriminant value
  properties: Record<string, TypeInfo>;
}

export interface EventTypeInfo {
  typeName: string;
  variants: EventVariant[];
  node: ts.TypeAliasDeclaration | null;
}

export interface StateTypeInfo {
  typeName: string;
  type: TypeInfo;
  node: ts.TypeAliasDeclaration | null;
}

export interface AggregateAnalysis {
  className: string;
  aggregateName: string;
  filePath: string;
  relativePath: string;
  stateType: StateTypeInfo;
  eventType: EventTypeInfo;
  commands: CommandInfo[];
  node: ts.ClassDeclaration;
}

// ============================================================================
// Orchestrator Analysis Types
// ============================================================================

/**
 * Dependency type detected from constructor parameter.
 * - "aggregate": Needs to be loaded from event store
 * - "adapter": Needs to be injected from App().adapter() registration
 */
export type OrchestratorDependencyKind = "aggregate" | "adapter";

/**
 * A dependency of an orchestrator, inferred from constructor parameters.
 */
export interface OrchestratorDependency {
  /** Parameter name (e.g., "order") */
  name: string;
  /** Type name (e.g., "OrderAggregate" or "StripeAdapter") */
  typeName: string;
  /** Kind of dependency */
  kind: OrchestratorDependencyKind;
  /** For aggregates: inferred ID param name (e.g., "orderId") */
  idParamName?: string;
  /** AST node for the parameter */
  node: ts.ParameterDeclaration;
}

/**
 * Parameter info for the orchestrate() method.
 */
export interface OrchestrateMethodParam {
  name: string;
  type: TypeInfo;
  optional: boolean;
  node: ts.ParameterDeclaration;
}

/**
 * Analysis result for an orchestrator class.
 */
export interface OrchestratorAnalysis {
  /** Class name (e.g., "CreatePaymentIntentOrchestrator") */
  className: string;
  /** Derived orchestrator name (e.g., "createPaymentIntent") */
  orchestratorName: string;
  /** Source file path */
  filePath: string;
  /** Relative path from domain dir */
  relativePath: string;
  /** Constructor dependencies */
  dependencies: OrchestratorDependency[];
  /** orchestrate() method parameters (can be object or separate params) */
  orchestrateParams: OrchestrateMethodParam[];
  /** Whether orchestrate params are a single object vs separate params */
  paramsStyle: "object" | "separate";
  /** AST node for the class */
  node: ts.ClassDeclaration;
}

/**
 * Combined input type for the generated handler.
 * Merges aggregate IDs + orchestrate() params.
 */
export interface OrchestratorHandlerInput {
  /** All parameters the handler needs */
  params: Array<{
    name: string;
    type: TypeInfo;
    optional: boolean;
    /** Where this param comes from */
    source: "aggregate-id" | "orchestrate-input";
  }>;
}

// ============================================================================
// Diagnostic Types
// ============================================================================

export type DiagnosticSeverity = "error" | "warning" | "info";

export interface DiagnosticLocation {
  filePath: string;
  line: number;
  column: number;
  endLine?: number;
  endColumn?: number;
}

export interface DiagnosticRelatedInfo {
  message: string;
  location: DiagnosticLocation;
}

export interface Diagnostic {
  code: string;
  severity: DiagnosticSeverity;
  message: string;
  location: DiagnosticLocation;
  suggestion?: string;
  relatedInfo?: DiagnosticRelatedInfo[];
}

// ============================================================================
// Validation Types
// ============================================================================

export interface ValidationResult {
  valid: boolean;
  diagnostics: Diagnostic[];
  aggregates: AggregateAnalysis[];
  orchestrators: OrchestratorAnalysis[];
}

// ============================================================================
// Generation Types
// ============================================================================

export interface GeneratedFile {
  path: string;
  content: string;
}

export interface GenerationResult {
  handlers: GeneratedFile[];
  validators: GeneratedFile[];
  orchestratorHandlers: GeneratedFile[];
  wiring: GeneratedFile | null;
  index: GeneratedFile | null;
  auth: GeneratedFile | null;
  routes: GeneratedFile | null;
}
