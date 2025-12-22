//! Project scaffolding generation for Bun/TypeScript.

use std::path::Path;

/// Generates package.json for the SpiteStack project.
/// If `spitedb_napi_path` is provided, uses a file: reference. Otherwise uses workspace:*.
pub fn generate_package_json(name: &str, spitedb_napi_path: Option<&str>) -> String {
    let db_dep = match spitedb_napi_path {
        Some(path) => format!("\"@spitestack/db\": \"file:{}\"", path),
        None => "\"@spitestack/db\": \"workspace:*\"".to_string(),
    };

    format!(
        r#"{{
  "name": "{}",
  "type": "module",
  "scripts": {{
    "dev": "bun run --hot src/index.ts",
    "build": "bun build src/index.ts --outdir dist --target bun",
    "start": "bun run dist/index.js",
    "typecheck": "tsc --noEmit"
  }},
  "dependencies": {{
    {}
  }},
  "devDependencies": {{
    "@types/bun": "latest",
    "typescript": "^5.0.0"
  }}
}}
"#,
        name, db_dep
    )
}

/// Detects if we're in the spitedb monorepo and returns the relative path to crates/spitedb-napi.
pub fn detect_napi_path(output_dir: &Path) -> Option<String> {
    let abs_output = output_dir.canonicalize().ok()?;

    // Walk up looking for workspace root (has package.json with workspaces)
    let mut current = abs_output.parent()?;
    while current.parent().is_some() {
        let pkg_json = current.join("package.json");
        if pkg_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&pkg_json) {
                if content.contains("\"workspaces\"") && content.contains("crates/spitedb-napi") {
                    // Found monorepo root
                    let napi_path = current.join("crates/spitedb-napi");
                    if napi_path.exists() {
                        // Compute relative path from output dir to napi
                        let rel_path = pathdiff::diff_paths(&napi_path, &abs_output)?;
                        return Some(rel_path.to_string_lossy().to_string());
                    }
                }
            }
        }
        current = current.parent()?;
    }
    None
}

/// Generates tsconfig.json for the project.
pub fn generate_tsconfig() -> &'static str {
    r#"{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "skipLibCheck": true,
    "noEmit": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "types": ["bun-types"]
  },
  "include": ["src/**/*"]
}
"#
}

/// Generates src/index.ts entry point.
pub fn generate_index_ts(port: u16, app_name: &str) -> String {
    format!(
        r#"import {{ SpiteDbNapi, TelemetryDbNapi }} from '@spitestack/db';
import {{ mkdir }} from 'node:fs/promises';
import {{ createRouter }} from './generated/router';

const eventsDir = './data/events';
const telemetryDir = './data/telemetry';

// Ensure data directories exist
await mkdir(eventsDir, {{ recursive: true }});
await mkdir(telemetryDir, {{ recursive: true }});

const db = await SpiteDbNapi.open(`${{eventsDir}}/{}.db`);
const telemetry = await TelemetryDbNapi.open(telemetryDir, {{ appName: '{}' }});
const router = createRouter({{ db, telemetry, tenant: 'default' }});

const server = Bun.serve({{
  port: {},
  fetch: router,
}});

console.log(`🚀 SpiteStack server running at http://localhost:${{server.port}}`);

// Best-effort startup telemetry
void telemetry.writeBatch([{{
  tsMs: Date.now(),
  kind: 'Log',
  tenantId: 'default',
  severity: 1,
  message: 'server.start',
  attrsJson: JSON.stringify({{
    port: server.port,
    env: process.env.NODE_ENV ?? 'dev',
  }}),
}}]).catch(() => {{}});

process.on('SIGINT', () => {{
  void telemetry.writeBatch([{{
    tsMs: Date.now(),
    kind: 'Log',
    tenantId: 'default',
    severity: 1,
    message: 'server.stop',
  }}]).finally(() => process.exit(0));
}});
"#,
        app_name, app_name, port
    )
}

/// Generates .gitignore for the project.
pub fn generate_gitignore() -> &'static str {
    r#"node_modules/
dist/
data/
*.db
*.db-shm
*.db-wal
.env
"#
}

/// Generates a README for the generated project.
pub fn generate_readme(name: &str) -> String {
    format!(
        r#"# {name}

This is a generated SpiteStack project. Do not edit files in `src/generated/` directly.

## Development

```bash
# Start the development server (with hot reload)
spitestack dev

# Or run manually
bun run dev
```

## Build

```bash
bun run build
bun run start
```

## Structure

- `src/index.ts` - Server entry point
- `src/generated/` - Generated domain code (do not edit)
- `../domain/` - Source TypeScript domain logic
"#,
        name = name
    )
}

/// Generates the generated/index.ts re-export file.
///
/// Re-exports user's source files (events, state, aggregate) and generated wiring (validators, handlers).
///
/// `domain_import_path` is the path from handlers/ directory. Since index.ts is one level up,
/// we need to adjust the path by removing one `..` prefix.
pub fn generate_generated_index(aggregates: &[String], domain_import_path: &str) -> String {
    let mut output = String::new();

    // Adjust path: handlers are in generated/handlers/, index is in generated/
    // So we need one less "../" in the path
    let adjusted_path = if domain_import_path.starts_with("../") {
        &domain_import_path[3..] // Remove first "../"
    } else {
        domain_import_path
    };

    output.push_str("// Re-export user's domain types and generated wiring\n\n");

    for agg in aggregates {
        let snake = to_snake_case(agg);
        // From user's source files
        output.push_str(&format!("export * from '{}/{}/events';\n", adjusted_path, agg));
        output.push_str(&format!("export * from '{}/{}/state';\n", adjusted_path, agg));
        output.push_str(&format!("export * from '{}/{}/aggregate';\n", adjusted_path, agg));
        // From generated wiring
        output.push_str(&format!("export * from './validators/{}.validator';\n", snake));
        output.push_str(&format!("export * from './handlers/{}.handlers';\n", snake));
    }

    output.push_str("\nexport * from './router';\n");

    output
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}
