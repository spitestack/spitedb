import ts from "typescript";
import { relative, dirname, basename } from "node:path";
import type {
  ParsedFile,
  OrchestratorAnalysis,
  OrchestratorDependency,
  OrchestratorDependencyKind,
  OrchestrateMethodParam,
  Diagnostic,
  TypeInfo,
} from "../types";
import { DiagnosticCode, DiagnosticMessages } from "../errors/codes";
import { getLineAndColumn, getEndLineAndColumn } from "./parse";
import { extractTypeInfo } from "./analyze";

/**
 * Check if a class is an orchestrator (name ends with "Orchestrator")
 */
function isOrchestratorClass(className: string): boolean {
  return className.endsWith("Orchestrator");
}

/**
 * Derive orchestrator name from class name (lowercase, without "Orchestrator" suffix)
 * e.g., CreatePaymentIntentOrchestrator -> createPaymentIntent
 */
function deriveOrchestratorName(className: string): string {
  let name = className;
  if (name.endsWith("Orchestrator")) {
    name = name.slice(0, -"Orchestrator".length);
  }
  // Convert to camelCase (first letter lowercase)
  return name.charAt(0).toLowerCase() + name.slice(1);
}

/**
 * Check if a type is an aggregate based on type analysis.
 * Detection methods:
 * 1. Type name ends with "Aggregate"
 * 2. Type has aggregate structure (emit, apply, events members)
 */
function isAggregateType(
  typeName: string,
  type: ts.Type,
  typeChecker: ts.TypeChecker
): boolean {
  // Check naming convention
  if (typeName.endsWith("Aggregate")) {
    return true;
  }

  // Check for aggregate structure
  const properties = type.getProperties();
  const propNames = new Set(properties.map((p) => p.getName()));

  // Aggregates have: emit, apply, events, currentState
  const hasEmit = propNames.has("emit");
  const hasApply = propNames.has("apply");
  const hasEvents = propNames.has("events");

  // If it has at least emit and apply, likely an aggregate
  if (hasEmit && hasApply) {
    return true;
  }

  return false;
}

/**
 * Infer the ID parameter name for an aggregate dependency.
 * e.g., order: OrderAggregate -> orderId
 */
function inferIdParamName(paramName: string): string {
  return `${paramName}Id`;
}

/**
 * Extract constructor dependencies from an orchestrator class.
 */
function extractConstructorDependencies(
  classDecl: ts.ClassDeclaration,
  typeChecker: ts.TypeChecker,
  sourceFile: ts.SourceFile
): OrchestratorDependency[] {
  const dependencies: OrchestratorDependency[] = [];

  // Find the constructor
  for (const member of classDecl.members) {
    if (!ts.isConstructorDeclaration(member)) continue;

    for (const param of member.parameters) {
      const paramName = param.name.getText(sourceFile);

      // Get the type name
      let typeName = "unknown";
      if (param.type && ts.isTypeReferenceNode(param.type)) {
        typeName = param.type.typeName.getText(sourceFile);
      }

      // Get the full type for analysis
      const type = typeChecker.getTypeAtLocation(param);

      // Determine if this is an aggregate or adapter
      const isAggregate = isAggregateType(typeName, type, typeChecker);
      const kind: OrchestratorDependencyKind = isAggregate ? "aggregate" : "adapter";

      const dependency: OrchestratorDependency = {
        name: paramName,
        typeName,
        kind,
        node: param,
      };

      // For aggregates, infer the ID param name
      if (isAggregate) {
        dependency.idParamName = inferIdParamName(paramName);
      }

      dependencies.push(dependency);
    }

    break; // Only process the first constructor
  }

  return dependencies;
}

/**
 * Find and analyze the orchestrate() method.
 */
function extractOrchestrateMethod(
  classDecl: ts.ClassDeclaration,
  typeChecker: ts.TypeChecker,
  sourceFile: ts.SourceFile
): {
  params: OrchestrateMethodParam[];
  paramsStyle: "object" | "separate";
  node: ts.MethodDeclaration | null;
} | null {
  for (const member of classDecl.members) {
    if (!ts.isMethodDeclaration(member)) continue;

    const methodName = member.name?.getText(sourceFile);
    if (methodName !== "orchestrate") continue;

    const params: OrchestrateMethodParam[] = [];

    // Check if single object param or multiple separate params
    const methodParams = member.parameters;

    if (methodParams.length === 0) {
      return { params: [], paramsStyle: "separate", node: member };
    }

    // If single param with object type, it's object style
    if (methodParams.length === 1) {
      const singleParam = methodParams[0];
      const paramType = singleParam.type;

      if (paramType && ts.isTypeLiteralNode(paramType)) {
        // Object style: orchestrate(input: { amount: number, currency: string })
        for (const propMember of paramType.members) {
          if (ts.isPropertySignature(propMember) && propMember.name) {
            const propName = propMember.name.getText(sourceFile);
            const propType = extractTypeInfo(propMember.type, typeChecker, sourceFile);
            const optional = propMember.questionToken !== undefined;

            params.push({
              name: propName,
              type: propType,
              optional,
              node: singleParam,
            });
          }
        }

        return { params, paramsStyle: "object", node: member };
      }

      // Check if it's a type reference to an interface
      if (paramType && ts.isTypeReferenceNode(paramType)) {
        const type = typeChecker.getTypeAtLocation(paramType);
        const properties = type.getProperties();

        for (const prop of properties) {
          const propType = typeChecker.getTypeOfSymbolAtLocation(prop, prop.valueDeclaration!);
          const typeInfo = extractTypeFromTypeObject(propType, typeChecker);

          params.push({
            name: prop.getName(),
            type: typeInfo,
            optional: (prop.flags & ts.SymbolFlags.Optional) !== 0,
            node: singleParam,
          });
        }

        return { params, paramsStyle: "object", node: member };
      }
    }

    // Multiple params = separate style: orchestrate(amount: number, currency: string)
    for (const param of methodParams) {
      const paramName = param.name.getText(sourceFile);
      const paramType = extractTypeInfo(param.type, typeChecker, sourceFile);
      const optional = param.questionToken !== undefined;

      params.push({
        name: paramName,
        type: paramType,
        optional,
        node: param,
      });
    }

    return { params, paramsStyle: "separate", node: member };
  }

  return null;
}

/**
 * Convert ts.Type to TypeInfo
 */
function extractTypeFromTypeObject(type: ts.Type, typeChecker: ts.TypeChecker): TypeInfo {
  // String
  if (type.flags & ts.TypeFlags.String) {
    return { kind: "string" };
  }

  // Number
  if (type.flags & ts.TypeFlags.Number) {
    return { kind: "number" };
  }

  // Boolean
  if (type.flags & ts.TypeFlags.Boolean || type.flags & ts.TypeFlags.BooleanLiteral) {
    return { kind: "boolean" };
  }

  // Null
  if (type.flags & ts.TypeFlags.Null) {
    return { kind: "null" };
  }

  // Undefined
  if (type.flags & ts.TypeFlags.Undefined) {
    return { kind: "undefined" };
  }

  // String/Number literals
  if (type.flags & ts.TypeFlags.StringLiteral) {
    return { kind: "literal", literalValue: (type as ts.StringLiteralType).value };
  }
  if (type.flags & ts.TypeFlags.NumberLiteral) {
    return { kind: "literal", literalValue: (type as ts.NumberLiteralType).value };
  }

  // Union types
  if (type.isUnion()) {
    const types = type.types.map((t) => extractTypeFromTypeObject(t, typeChecker));
    return { kind: "union", types };
  }

  // Object types
  if (type.flags & ts.TypeFlags.Object) {
    const objectType = type as ts.ObjectType;

    // Check for array
    const symbol = type.getSymbol();
    if (symbol?.getName() === "Array") {
      const typeArgs = typeChecker.getTypeArguments(objectType as ts.TypeReference);
      if (typeArgs.length > 0) {
        return {
          kind: "array",
          elementType: extractTypeFromTypeObject(typeArgs[0], typeChecker),
        };
      }
    }

    // Regular object
    const properties: Record<string, TypeInfo> = {};
    const props = type.getProperties();

    for (const prop of props) {
      const propType = typeChecker.getTypeOfSymbolAtLocation(prop, prop.valueDeclaration!);
      properties[prop.getName()] = extractTypeFromTypeObject(propType, typeChecker);
    }

    return { kind: "object", properties };
  }

  return { kind: "unknown" };
}

/**
 * Analyze a single class declaration as an orchestrator.
 */
export function analyzeOrchestrator(
  classDecl: ts.ClassDeclaration,
  parsedFile: ParsedFile,
  typeChecker: ts.TypeChecker,
  domainDir: string
): { orchestrator: OrchestratorAnalysis | null; diagnostics: Diagnostic[] } {
  const diagnostics: Diagnostic[] = [];
  const sourceFile = parsedFile.sourceFile;
  const className = classDecl.name?.getText(sourceFile);

  if (!className) {
    return { orchestrator: null, diagnostics };
  }

  // Check if class name ends with "Orchestrator"
  if (!isOrchestratorClass(className)) {
    return { orchestrator: null, diagnostics };
  }

  // Derive orchestrator name
  const orchestratorName = deriveOrchestratorName(className);

  // Helper to create diagnostics
  const createDiag = (
    code: keyof typeof DiagnosticCode,
    node: ts.Node,
    message?: string
  ): Diagnostic => {
    const { line, column } = getLineAndColumn(sourceFile, node.getStart());
    const { endLine, endColumn } = getEndLineAndColumn(sourceFile, node);

    return {
      code: DiagnosticCode[code],
      severity: "error",
      message: message || DiagnosticMessages[DiagnosticCode[code]],
      location: {
        filePath: parsedFile.filePath,
        line,
        column,
        endLine,
        endColumn,
      },
    };
  };

  // Extract constructor dependencies
  const dependencies = extractConstructorDependencies(classDecl, typeChecker, sourceFile);

  // Extract orchestrate method
  const orchestrateResult = extractOrchestrateMethod(classDecl, typeChecker, sourceFile);

  if (!orchestrateResult) {
    const { line, column } = getLineAndColumn(sourceFile, classDecl.getStart());
    diagnostics.push({
      code: "ORCH001",
      severity: "error",
      message: `Orchestrator '${className}' must have an 'orchestrate' method`,
      location: { filePath: parsedFile.filePath, line, column },
      suggestion: `Add an orchestrate method:\n\nasync orchestrate(/* params */): Promise<void> {\n  // orchestration logic\n}`,
    });

    return { orchestrator: null, diagnostics };
  }

  // Warn if no dependencies found
  if (dependencies.length === 0) {
    const { line, column } = getLineAndColumn(sourceFile, classDecl.getStart());
    diagnostics.push({
      code: "ORCH002",
      severity: "warning",
      message: `Orchestrator '${className}' has no constructor dependencies`,
      location: { filePath: parsedFile.filePath, line, column },
    });
  }

  const orchestrator: OrchestratorAnalysis = {
    className,
    orchestratorName,
    filePath: parsedFile.filePath,
    relativePath: relative(domainDir, parsedFile.filePath),
    dependencies,
    orchestrateParams: orchestrateResult.params,
    paramsStyle: orchestrateResult.paramsStyle,
    node: classDecl,
  };

  return { orchestrator, diagnostics };
}

/**
 * Analyze all parsed files and extract orchestrator information.
 */
export function analyzeOrchestratorFiles(
  parsedFiles: ParsedFile[],
  typeChecker: ts.TypeChecker,
  domainDir: string
): { orchestrators: OrchestratorAnalysis[]; diagnostics: Diagnostic[] } {
  const orchestrators: OrchestratorAnalysis[] = [];
  const diagnostics: Diagnostic[] = [];

  for (const parsedFile of parsedFiles) {
    for (const classDecl of parsedFile.classes) {
      const result = analyzeOrchestrator(classDecl, parsedFile, typeChecker, domainDir);

      if (result.orchestrator) {
        orchestrators.push(result.orchestrator);
      }

      diagnostics.push(...result.diagnostics);
    }
  }

  return { orchestrators, diagnostics };
}

/**
 * Analyze orchestrators with knowledge of existing aggregates.
 * This allows cross-referencing aggregate dependencies.
 */
export function analyzeOrchestrators(
  parsedFiles: ParsedFile[],
  typeChecker: ts.TypeChecker,
  program: ts.Program,
  domainDir: string,
  aggregates: { className: string; aggregateName: string }[]
): { orchestrators: OrchestratorAnalysis[]; diagnostics: Diagnostic[] } {
  // Use the base analysis function
  const result = analyzeOrchestratorFiles(parsedFiles, typeChecker, domainDir);

  // Build a set of known aggregate class names for validation
  const knownAggregates = new Set(aggregates.map((a) => a.className));

  // Validate that aggregate dependencies reference known aggregates
  for (const orchestrator of result.orchestrators) {
    for (const dep of orchestrator.dependencies) {
      if (dep.kind === "aggregate" && !knownAggregates.has(dep.typeName)) {
        result.diagnostics.push({
          code: "ORCH003",
          severity: "warning",
          message: `Orchestrator '${orchestrator.className}' references unknown aggregate '${dep.typeName}'`,
          location: {
            filePath: orchestrator.filePath,
            line: 1,
            column: 1,
          },
          suggestion: `Make sure the aggregate is defined in the domain directory`,
        });
      }
    }
  }

  return result;
}
