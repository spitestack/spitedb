/**
 * Projection Runner
 *
 * Manages the event processing loop for projections.
 * Fetches batches of events from Rust, runs the user's apply function,
 * and sends operations back to be persisted atomically.
 */
import { createProjectionProxy } from './proxy';
/**
 * Converts a schema definition to NAPI column definitions.
 */
function schemaToColumnDefs(schema) {
    const columns = [];
    for (const [name, def] of Object.entries(schema)) {
        if (typeof def === 'string') {
            // Simple type definition
            columns.push({
                name,
                colType: def,
                primaryKey: false,
                nullable: true,
                defaultValue: undefined,
            });
        }
        else {
            // Full definition
            columns.push({
                name,
                colType: def.type,
                primaryKey: def.primaryKey ?? false,
                nullable: def.nullable ?? true,
                defaultValue: def.defaultValue !== undefined
                    ? JSON.stringify(def.defaultValue)
                    : undefined,
            });
        }
    }
    return columns;
}
/**
 * Finds the primary key column name from a schema.
 */
function findPrimaryKey(schema) {
    for (const [name, def] of Object.entries(schema)) {
        if (typeof def === 'object' && def.primaryKey) {
            return name;
        }
    }
    // Default to first column if no explicit primary key
    return Object.keys(schema)[0];
}
/**
 * Converts a NAPI event to a ProjectionEvent.
 */
function napiEventToProjectionEvent(event) {
    return {
        globalPos: BigInt(event.globalPos),
        streamId: event.streamId,
        streamRev: BigInt(event.streamRev),
        timestampMs: BigInt(event.timestampMs),
        data: event.data,
    };
}
/**
 * Projection runner that manages the event processing loop.
 */
export class ProjectionRunner {
    native;
    projections = new Map();
    initialized = false;
    constructor(native) {
        this.native = native;
    }
    /**
     * Initializes the projection storage.
     * Must be called before registering projections.
     */
    async init(projectionDbPath) {
        await this.native.initProjections(projectionDbPath);
        this.initialized = true;
    }
    /**
     * Registers a projection.
     * Creates the table if it doesn't exist.
     */
    async register(name, options) {
        if (!this.initialized) {
            throw new Error('ProjectionRunner not initialized. Call init() first.');
        }
        const columns = schemaToColumnDefs(options.schema);
        await this.native.registerProjection(name, columns);
        const primaryKeyColumn = findPrimaryKey(options.schema);
        this.projections.set(name, {
            name,
            options: options,
            primaryKeyColumn,
            running: false,
            abortController: new AbortController(),
        });
    }
    /**
     * Starts processing events for all registered projections.
     */
    async startAll() {
        const promises = [];
        for (const state of this.projections.values()) {
            if (!state.running) {
                promises.push(this.startProjection(state));
            }
        }
        await Promise.all(promises);
    }
    /**
     * Starts processing events for a specific projection.
     */
    async startProjection(state) {
        state.running = true;
        state.abortController = new AbortController();
        const batchSize = state.options.batchSize ?? 100;
        try {
            while (state.running) {
                // Check if aborted
                if (state.abortController.signal.aborted) {
                    break;
                }
                // Fetch next batch of events
                const batch = await this.native.getProjectionEvents(state.name, batchSize);
                if (batch === null) {
                    // No more events, wait a bit and try again
                    await sleep(100);
                    continue;
                }
                // Process the batch
                await this.processBatch(state, batch);
            }
        }
        catch (error) {
            console.error(`Projection ${state.name} error:`, error);
            state.running = false;
            throw error;
        }
    }
    /**
     * Processes a batch of events.
     */
    async processBatch(state, batch) {
        const { proxy, flush } = createProjectionProxy(state.name, state.primaryKeyColumn, this.native);
        let lastGlobalPos = batch.batchId;
        for (const eventNapi of batch.events) {
            const event = napiEventToProjectionEvent(eventNapi);
            try {
                // Run the user's apply function
                await state.options.apply(event, proxy);
                lastGlobalPos = eventNapi.globalPos;
            }
            catch (error) {
                // Handle error based on user's strategy
                const strategy = state.options.onError?.(error, event) ?? 'stop';
                switch (strategy) {
                    case 'skip':
                        // Skip this event and continue
                        lastGlobalPos = eventNapi.globalPos;
                        continue;
                    case 'retry':
                        // Retry the same event
                        try {
                            await state.options.apply(event, proxy);
                            lastGlobalPos = eventNapi.globalPos;
                        }
                        catch {
                            // If retry fails, stop
                            throw error;
                        }
                        break;
                    case 'stop':
                    default:
                        throw error;
                }
            }
        }
        // Flush operations and apply to database
        const operations = flush();
        const result = {
            projectionName: state.name,
            operations: operations.map((op) => ({
                opType: op.opType,
                key: op.key,
                value: op.value,
            })),
            lastGlobalPos,
        };
        await this.native.applyProjectionBatch(result);
    }
    /**
     * Stops processing for all projections.
     */
    stopAll() {
        for (const state of this.projections.values()) {
            state.running = false;
            state.abortController.abort();
        }
    }
    /**
     * Stops processing for a specific projection.
     */
    stop(name) {
        const state = this.projections.get(name);
        if (state) {
            state.running = false;
            state.abortController.abort();
        }
    }
}
/**
 * Simple sleep helper.
 */
function sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
}
