//! Projection code generation.
//!
//! Generates:
//! - SQLite schema for projection tables
//! - Bun worker code for each projection
//! - Query handlers for HTTP endpoints

use crate::ir::{ProjectionIR, ProjectionKind, DomainIR};
use super::ts_types::{to_snake_case, to_pascal_case};

/// Generates all projection-related code for a domain.
pub fn generate_projections(domain: &DomainIR, domain_import_path: &str) -> Vec<(String, String)> {
    let mut files = Vec::new();

    for projection in &domain.projections {
        let snake_name = to_snake_case(&projection.name);

        // Generate worker code
        let worker_code = generate_projection_worker(projection, domain_import_path);
        files.push((
            format!("projections/{}.worker.ts", snake_name),
            worker_code,
        ));

        // Generate handlers for query methods
        let handler_code = generate_projection_handlers(projection);
        files.push((
            format!("handlers/{}.projection.ts", snake_name),
            handler_code,
        ));

        // Generate SQL schema
        let schema_sql = generate_projection_schema(projection);
        files.push((
            format!("schemas/{}.sql", snake_name),
            schema_sql,
        ));
    }

    // Generate projection manager (starts all workers)
    if !domain.projections.is_empty() {
        let manager_code = generate_projection_manager(domain);
        files.push(("projections/manager.ts".to_string(), manager_code));
    }

    files
}

/// Generates SQL CREATE TABLE statement for a projection.
pub fn generate_projection_schema(projection: &ProjectionIR) -> String {
    let table_name = to_snake_case(&projection.name);
    let mut sql = String::new();

    sql.push_str(&format!("-- Schema for {} projection\n", projection.name));
    sql.push_str(&format!("-- Kind: {:?}\n\n", projection.kind));

    // Main table
    sql.push_str(&format!("CREATE TABLE IF NOT EXISTS {} (\n", table_name));
    sql.push_str("    tenant_id TEXT NOT NULL,\n");

    // Primary keys
    for pk in &projection.schema.primary_keys {
        sql.push_str(&format!("    {} {} NOT NULL,\n", pk.name, pk.sql_type.to_sql()));
    }

    // Data columns
    for col in &projection.schema.columns {
        let nullable = if col.nullable { "" } else { " NOT NULL" };
        let default = col.default.as_ref()
            .map(|d| format!(" DEFAULT {}", d))
            .unwrap_or_default();
        sql.push_str(&format!("    {} {}{}{},\n", col.name, col.sql_type.to_sql(), nullable, default));
    }

    // Timestamps
    sql.push_str("    created_at TEXT NOT NULL DEFAULT (datetime('now')),\n");
    sql.push_str("    updated_at TEXT NOT NULL DEFAULT (datetime('now')),\n");

    // Primary key
    let pk_cols: Vec<_> = std::iter::once("tenant_id".to_string())
        .chain(projection.schema.primary_keys.iter().map(|pk| pk.name.clone()))
        .collect();
    sql.push_str(&format!("    PRIMARY KEY ({})\n", pk_cols.join(", ")));
    sql.push_str(");\n\n");

    // Indexes
    for idx in &projection.schema.indexes {
        let unique = if idx.unique { "UNIQUE " } else { "" };
        sql.push_str(&format!(
            "CREATE {}INDEX IF NOT EXISTS {}_{} ON {} (tenant_id, {});\n",
            unique,
            table_name,
            idx.name,
            table_name,
            idx.columns.join(", ")
        ));
    }

    // Position tracking table (for resumable processing)
    sql.push_str(&format!("\n-- Position tracking for {}\n", projection.name));
    sql.push_str(&format!("CREATE TABLE IF NOT EXISTS {}_position (\n", table_name));
    sql.push_str("    tenant_id TEXT PRIMARY KEY,\n");
    sql.push_str("    last_event_id INTEGER NOT NULL DEFAULT 0,\n");
    sql.push_str("    updated_at TEXT NOT NULL DEFAULT (datetime('now'))\n");
    sql.push_str(");\n");

    sql
}

/// Generates the Bun worker code for a projection.
fn generate_projection_worker(projection: &ProjectionIR, domain_import_path: &str) -> String {
    let name = &projection.name;
    let snake_name = to_snake_case(name);
    let pascal_name = to_pascal_case(name);

    // Get subscribed event types for import
    let event_imports: Vec<String> = projection.subscribed_events
        .iter()
        .filter_map(|e| e.aggregate.as_ref().map(|a| (a.clone(), e.event_name.clone())))
        .fold(std::collections::HashMap::<String, Vec<String>>::new(), |mut acc, (agg, evt)| {
            acc.entry(agg).or_default().push(evt);
            acc
        })
        .into_iter()
        .map(|(agg, _events)| format!("import type {{ {}Event }} from '{}/{}/events';", agg, domain_import_path, agg))
        .collect();

    let kind_comment = match projection.kind {
        ProjectionKind::DenormalizedView => "SQLite-backed row storage",
        ProjectionKind::Aggregator => "Memory-resident with checkpointing",
        ProjectionKind::TimeSeries => "Time-bucketed data with range queries",
    };

    let state_property = &projection.schema.state_property_name;

    let primary_key_columns = projection.schema.primary_keys
        .iter()
        .map(|pk| format!("{} {} NOT NULL,", pk.name, pk.sql_type.to_sql()))
        .collect::<Vec<_>>()
        .join("\n                ");

    let data_columns = projection.schema.columns
        .iter()
        .map(|col| {
            let nullable = if col.nullable { "" } else { " NOT NULL" };
            format!("{} {}{},", col.name, col.sql_type.to_sql(), nullable)
        })
        .collect::<Vec<_>>()
        .join("\n                ");

    let primary_keys = projection.schema.primary_keys
        .iter()
        .map(|pk| pk.name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    let index_creation = projection.schema.indexes
        .iter()
        .map(|idx| format!(
            "this.db.run('CREATE INDEX IF NOT EXISTS {}_{} ON {} (tenant_id, {})');",
            snake_name, idx.name, snake_name, idx.columns.join(", ")
        ))
        .collect::<Vec<_>>()
        .join("\n        ");

    let subscribed_events_list = projection.subscribed_events
        .iter()
        .map(|e| format!("'{}'", e.event_name))
        .collect::<Vec<_>>()
        .join(", ");

    let persist_logic = generate_persist_logic(projection);
    let query_methods = generate_worker_query_methods(projection);

    format!(
        r#"/**
 * Projection Worker: {name}
 * Kind: {kind_comment}
 *
 * This worker runs in a separate Bun process and polls the event log
 * to build and maintain the projection state.
 *
 * @generated by spitestack compiler
 */

import {{ Database }} from 'bun:sqlite';
import {{ SpiteDbNapi }} from '@spitestack/db';
{event_imports}

// Import the projection class from domain
import {{ {name} }} from '{domain_import_path}/{name}/projection';

// Configuration (can be overridden via environment)
const POLL_INTERVAL_MS = parseInt(process.env.PROJECTION_POLL_INTERVAL ?? '50');
const BATCH_SIZE = parseInt(process.env.PROJECTION_BATCH_SIZE ?? '100');
const DATA_DIR = process.env.PROJECTION_DATA_DIR ?? './data/projections';

// Event types this projection subscribes to
const SUBSCRIBED_EVENTS = [{subscribed_events_list}];

class {pascal_name}Worker {{
    private db: Database;
    private eventDb: SpiteDbNapi;
    private projection: {name};
    private running = false;
    private tenant: string;

    constructor(tenant: string, eventDbPath: string) {{
        this.tenant = tenant;
        this.projection = new {name}();

        // Open projection SQLite database
        const projectionDbPath = `${{DATA_DIR}}/{snake_name}_${{tenant}}.db`;
        this.db = new Database(projectionDbPath);
        this.db.run('PRAGMA journal_mode = WAL');

        // Initialize schema
        this.initSchema();

        // Connect to event store
        this.eventDb = new SpiteDbNapi(eventDbPath);
    }}

    private initSchema(): void {{
        // Create projection table if not exists
        this.db.run(`
            CREATE TABLE IF NOT EXISTS {snake_name} (
                tenant_id TEXT NOT NULL,
                {primary_key_columns}
                {data_columns}
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (tenant_id, {primary_keys})
            )
        `);

        // Create indexes
        {index_creation}

        // Create position tracking table
        this.db.run(`
            CREATE TABLE IF NOT EXISTS {snake_name}_position (
                tenant_id TEXT PRIMARY KEY,
                last_event_id INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        `);
    }}

    private getLastPosition(): number {{
        const row = this.db.query(
            'SELECT last_event_id FROM {snake_name}_position WHERE tenant_id = ?'
        ).get(this.tenant) as {{ last_event_id: number }} | null;
        return row?.last_event_id ?? 0;
    }}

    private updatePosition(eventId: number): void {{
        this.db.run(`
            INSERT INTO {snake_name}_position (tenant_id, last_event_id, updated_at)
            VALUES (?, ?, datetime('now'))
            ON CONFLICT(tenant_id) DO UPDATE SET
                last_event_id = excluded.last_event_id,
                updated_at = excluded.updated_at
        `, [this.tenant, eventId]);
    }}

    async start(): Promise<void> {{
        this.running = true;
        console.log(`[{name}] Starting projection worker for tenant: ${{this.tenant}}`);

        while (this.running) {{
            try {{
                await this.processBatch();
            }} catch (err) {{
                console.error(`[{name}] Error processing batch:`, err);
            }}
            await Bun.sleep(POLL_INTERVAL_MS);
        }}
    }}

    stop(): void {{
        this.running = false;
        console.log(`[{name}] Stopping projection worker`);
    }}

    private async processBatch(): Promise<void> {{
        const lastPosition = this.getLastPosition();

        // Read events from all subscribed streams
        const events = await this.eventDb.readGlobalEvents(
            lastPosition,
            BATCH_SIZE,
            this.tenant
        );

        if (events.length === 0) {{
            return;
        }}

        // Filter to subscribed events and process
        let maxEventId = lastPosition;
        const transaction = this.db.transaction(() => {{
            for (const event of events) {{
                const eventData = JSON.parse(event.data.toString());

                // Check if we're subscribed to this event type
                if (SUBSCRIBED_EVENTS.length === 0 || SUBSCRIBED_EVENTS.includes(eventData.type)) {{
                    // Apply event to projection
                    this.projection.build(eventData);

                    // Persist state changes
                    this.persistState();
                }}

                maxEventId = Math.max(maxEventId, event.id);
            }}

            // Update position
            this.updatePosition(maxEventId);
        }});

        transaction();

        if (events.length > 0) {{
            console.log(`[{name}] Processed ${{events.length}} events, position: ${{maxEventId}}`);
        }}
    }}

    private persistState(): void {{
        // Persist the projection state to SQLite
        const state = this.projection.{state_property};

        {persist_logic}
    }}

    // Query methods (called by handlers)
    {query_methods}
}}

// Worker entry point
const tenant = process.env.TENANT ?? 'default';
const eventDbPath = process.env.EVENT_DB_PATH ?? './data/events.db';

const worker = new {pascal_name}Worker(tenant, eventDbPath);

// Handle graceful shutdown
process.on('SIGTERM', () => worker.stop());
process.on('SIGINT', () => worker.stop());

// Start the worker
worker.start();

// Export for testing
export {{ {pascal_name}Worker }};
"#,
        name = name,
        event_imports = event_imports.join("\n"),
        snake_name = snake_name,
        pascal_name = pascal_name,
        domain_import_path = domain_import_path,
        kind_comment = kind_comment,
        state_property = state_property,
        subscribed_events_list = subscribed_events_list,
        primary_key_columns = primary_key_columns,
        data_columns = data_columns,
        primary_keys = primary_keys,
        index_creation = index_creation,
        persist_logic = persist_logic,
        query_methods = query_methods,
    )
}

/// Generates the persist logic based on projection kind.
fn generate_persist_logic(projection: &ProjectionIR) -> String {
    let snake_name = to_snake_case(&projection.name);

    match projection.kind {
        ProjectionKind::DenormalizedView => {
            let pk_name = projection.schema.primary_keys.first()
                .map(|pk| pk.name.clone())
                .unwrap_or_else(|| "id".to_string());

            let col_names: Vec<_> = projection.schema.columns
                .iter()
                .map(|c| c.name.clone())
                .collect();

            let col_values: Vec<_> = projection.schema.columns
                .iter()
                .map(|c| format!("row.{}", c.name))
                .collect();

            format!(
                r#"for (const [key, row] of Object.entries(state)) {{
            const stmt = this.db.prepare(`
                INSERT INTO {table} (tenant_id, {pk}, {cols}, updated_at)
                VALUES (?, ?, {placeholders}, datetime('now'))
                ON CONFLICT(tenant_id, {pk}) DO UPDATE SET
                    {updates},
                    updated_at = datetime('now')
            `);
            stmt.run(this.tenant, key, {values});
        }}"#,
                table = snake_name,
                pk = pk_name,
                cols = col_names.join(", "),
                placeholders = col_names.iter().map(|_| "?").collect::<Vec<_>>().join(", "),
                updates = col_names.iter().map(|c| format!("{} = excluded.{}", c, c)).collect::<Vec<_>>().join(", "),
                values = col_values.join(", "),
            )
        }
        ProjectionKind::Aggregator => {
            format!(
                r#"// Aggregator: persist as JSON checkpoint
        const stmt = this.db.prepare(`
            INSERT INTO {table} (tenant_id, id, state_json, updated_at)
            VALUES (?, 'singleton', ?, datetime('now'))
            ON CONFLICT(tenant_id, id) DO UPDATE SET
                state_json = excluded.state_json,
                updated_at = datetime('now')
        `);
        stmt.run(this.tenant, JSON.stringify(state));"#,
                table = snake_name,
            )
        }
        ProjectionKind::TimeSeries => {
            format!(
                r#"// Time-Series: persist each bucket
        for (const [timeKey, value] of Object.entries(state)) {{
            const stmt = this.db.prepare(`
                INSERT INTO {table} (tenant_id, time_key, value, updated_at)
                VALUES (?, ?, ?, datetime('now'))
                ON CONFLICT(tenant_id, time_key) DO UPDATE SET
                    value = excluded.value,
                    updated_at = datetime('now')
            `);
            stmt.run(this.tenant, timeKey, value);
        }}"#,
                table = snake_name,
            )
        }
    }
}

/// Generates query methods for the worker.
fn generate_worker_query_methods(projection: &ProjectionIR) -> String {
    projection.queries
        .iter()
        .map(|q| {
            let params = q.parameters
                .iter()
                .map(|p| format!("{}: {}", p.name, domain_type_to_ts(&p.typ)))
                .collect::<Vec<_>>()
                .join(", ");

            let snake_name = to_snake_case(&projection.name);

            if q.is_range_query {
                // Range query for time-series
                format!(
                    r#"
    {}({}): any[] {{
        const stmt = this.db.prepare(
            'SELECT * FROM {} WHERE tenant_id = ? AND time_key >= ? AND time_key <= ? ORDER BY time_key'
        );
        return stmt.all(this.tenant, start, end);
    }}"#,
                    q.name,
                    params,
                    snake_name,
                )
            } else {
                // Point query
                let where_clauses: Vec<_> = q.parameters
                    .iter()
                    .map(|p| format!("{} = ?", to_snake_case(&p.name)))
                    .collect();
                let param_names: Vec<_> = q.parameters
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();

                format!(
                    r#"
    {}({}): any | null {{
        const stmt = this.db.prepare(
            'SELECT * FROM {} WHERE tenant_id = ?{}'
        );
        return stmt.get(this.tenant{});
    }}"#,
                    q.name,
                    params,
                    snake_name,
                    if where_clauses.is_empty() { String::new() } else { format!(" AND {}", where_clauses.join(" AND ")) },
                    if param_names.is_empty() { String::new() } else { format!(", {}", param_names.join(", ")) },
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Converts domain type to TypeScript type string.
fn domain_type_to_ts(typ: &crate::ir::DomainType) -> &'static str {
    match typ {
        crate::ir::DomainType::String => "string",
        crate::ir::DomainType::Number => "number",
        crate::ir::DomainType::Boolean => "boolean",
        _ => "any",
    }
}

/// Generates HTTP handlers for projection queries.
fn generate_projection_handlers(projection: &ProjectionIR) -> String {
    let name = &projection.name;
    let snake_name = to_snake_case(name);

    let mut code = String::new();

    code.push_str(&format!(
        r#"/**
 * HTTP Handlers for {} Projection
 *
 * @generated by spitestack compiler
 */

import {{ Database }} from 'bun:sqlite';
import type {{ TelemetryDbNapi }} from '@spitestack/db';

const DATA_DIR = process.env.PROJECTION_DATA_DIR ?? './data/projections';

export type ProjectionHandlerContext = {{
    tenant: string;
    telemetry: TelemetryDbNapi;
}};

function getProjectionDb(tenant: string): Database {{
    const dbPath = `${{DATA_DIR}}/{}_${{tenant}}.db`;
    const db = new Database(dbPath, {{ readonly: true }});
    return db;
}}
"#,
        name,
        snake_name
    ));

    // Generate handler for each query method
    for query in &projection.queries {
        code.push_str(&generate_query_handler(projection, query));
    }

    code
}

/// Generates a single query handler.
fn generate_query_handler(projection: &ProjectionIR, query: &crate::ir::QueryMethodIR) -> String {
    let proj_name = &projection.name;
    let query_pascal = to_pascal_case(&query.name);
    let snake_name = to_snake_case(proj_name);

    let params_type = if query.parameters.is_empty() {
        String::new()
    } else {
        let fields: Vec<String> = query.parameters
            .iter()
            .map(|p| format!("{}: {}", p.name, domain_type_to_ts(&p.typ)))
            .collect();
        format!("params: {{ {} }}", fields.join(", "))
    };

    let param_args = if query.parameters.is_empty() {
        "ctx: ProjectionHandlerContext".to_string()
    } else {
        format!("ctx: ProjectionHandlerContext, {}", params_type)
    };

    if query.is_range_query {
        // Range query handler
        format!(
            r#"
export async function handle{proj_name}{query_pascal}(
    {param_args}
): Promise<Response> {{
    try {{
        const db = getProjectionDb(ctx.tenant);
        const stmt = db.prepare(
            'SELECT * FROM {table} WHERE tenant_id = ? AND time_key >= ? AND time_key <= ? ORDER BY time_key'
        );
        const rows = stmt.all(ctx.tenant, params.start ?? params.startDate ?? params.from, params.end ?? params.endDate ?? params.to);
        db.close();

        const total = rows.reduce((sum: number, r: any) => sum + (r.value ?? 0), 0);
        return new Response(JSON.stringify({{ total, rows }}), {{
            status: 200,
            headers: {{ 'Content-Type': 'application/json' }},
        }});
    }} catch (err) {{
        return new Response(JSON.stringify({{ error: (err as Error).message }}), {{
            status: 500,
            headers: {{ 'Content-Type': 'application/json' }},
        }});
    }}
}}
"#,
            proj_name = proj_name,
            query_pascal = query_pascal,
            param_args = param_args,
            table = snake_name,
        )
    } else {
        // Point query handler
        let where_clauses: Vec<String> = query.parameters
            .iter()
            .map(|p| format!("{} = ?", to_snake_case(&p.name)))
            .collect();

        let where_clause = if where_clauses.is_empty() {
            String::new()
        } else {
            format!(" AND {}", where_clauses.join(" AND "))
        };

        let param_values: Vec<String> = query.parameters
            .iter()
            .map(|p| format!("params.{}", p.name))
            .collect();

        let param_list = if param_values.is_empty() {
            "ctx.tenant".to_string()
        } else {
            format!("ctx.tenant, {}", param_values.join(", "))
        };

        format!(
            r#"
export async function handle{proj_name}{query_pascal}(
    {param_args}
): Promise<Response> {{
    try {{
        const db = getProjectionDb(ctx.tenant);
        const stmt = db.prepare(
            'SELECT * FROM {table} WHERE tenant_id = ?{where_clause}'
        );
        const row = stmt.get({param_list});
        db.close();

        if (!row) {{
            return new Response(JSON.stringify({{ error: 'Not found' }}), {{
                status: 404,
                headers: {{ 'Content-Type': 'application/json' }},
            }});
        }}

        return new Response(JSON.stringify(row), {{
            status: 200,
            headers: {{ 'Content-Type': 'application/json' }},
        }});
    }} catch (err) {{
        return new Response(JSON.stringify({{ error: (err as Error).message }}), {{
            status: 500,
            headers: {{ 'Content-Type': 'application/json' }},
        }});
    }}
}}
"#,
            proj_name = proj_name,
            query_pascal = query_pascal,
            param_args = param_args,
            table = snake_name,
            where_clause = where_clause,
            param_list = param_list,
        )
    }
}

/// Generates the projection manager that starts all workers.
fn generate_projection_manager(domain: &DomainIR) -> String {
    let worker_spawns: Vec<String> = domain.projections
        .iter()
        .map(|p| {
            let snake_name = to_snake_case(&p.name);
            format!(
                r#"    // Start {} worker
    const {snake_name}Worker = Bun.spawn({{
        cmd: ['bun', 'run', './_generated/projections/{snake_name}.worker.ts'],
        env: {{
            ...process.env,
            TENANT: tenant,
            EVENT_DB_PATH: eventDbPath,
            PROJECTION_DATA_DIR: dataDir,
        }},
        stdout: 'inherit',
        stderr: 'inherit',
    }});
    workers.push({{ name: '{name}', proc: {snake_name}Worker }});"#,
                p.name,
                snake_name = snake_name,
                name = p.name
            )
        })
        .collect();

    format!(
        r#"/**
 * Projection Manager
 *
 * Starts and manages all projection workers as separate Bun processes.
 * Each projection runs in isolation for parallel processing.
 *
 * @generated by spitestack compiler
 */

export interface ProjectionManagerConfig {{
    tenant: string;
    eventDbPath: string;
    dataDir?: string;
}}

interface WorkerHandle {{
    name: string;
    proc: ReturnType<typeof Bun.spawn>;
}}

const workers: WorkerHandle[] = [];

export function startProjections(config: ProjectionManagerConfig): void {{
    const {{ tenant, eventDbPath, dataDir = './data/projections' }} = config;

    console.log('[ProjectionManager] Starting projection workers...');

{worker_spawns}

    console.log(`[ProjectionManager] Started ${{workers.length}} projection workers`);
}}

export function stopProjections(): void {{
    console.log('[ProjectionManager] Stopping projection workers...');

    for (const worker of workers) {{
        console.log(`[ProjectionManager] Stopping ${{worker.name}}...`);
        worker.proc.kill();
    }}

    workers.length = 0;
    console.log('[ProjectionManager] All workers stopped');
}}

// Handle graceful shutdown
process.on('SIGTERM', stopProjections);
process.on('SIGINT', stopProjections);
"#,
        worker_spawns = worker_spawns.join("\n\n")
    )
}
