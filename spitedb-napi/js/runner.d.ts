/**
 * Projection Runner
 *
 * Manages the event processing loop for projections.
 * Fetches batches of events from Rust, runs the user's apply function,
 * and sends operations back to be persisted atomically.
 */
import type { NativeBinding } from './proxy';
import type { ProjectionOptions, SchemaDefinition } from './types';
/** Extended native binding interface */
export interface ProjectionNativeBinding extends NativeBinding {
    /** Initialize projection storage at the given path */
    initProjections(path: string): Promise<void>;
    /** Register a projection with its schema */
    registerProjection(name: string, schema: ColumnDefNapi[]): Promise<void>;
    /** Get the next batch of events for a projection */
    getProjectionEvents(projectionName: string, batchSize: number): Promise<EventBatchNapi | null>;
    /** Apply a batch of operations and update checkpoint */
    applyProjectionBatch(batch: BatchResultNapi): Promise<void>;
    /** Get current checkpoint for a projection */
    getProjectionCheckpoint(projectionName: string): Promise<number | null>;
}
/** NAPI types (matching Rust definitions) */
interface ColumnDefNapi {
    name: string;
    colType: string;
    primaryKey: boolean;
    nullable: boolean;
    defaultValue?: string;
}
interface EventNapi {
    globalPos: number;
    streamId: string;
    streamRev: number;
    timestampMs: number;
    data: Buffer;
}
interface EventBatchNapi {
    projectionName: string;
    events: EventNapi[];
    batchId: number;
}
interface ProjectionOpNapi {
    opType: string;
    key: string;
    value?: string;
}
interface BatchResultNapi {
    projectionName: string;
    operations: ProjectionOpNapi[];
    lastGlobalPos: number;
}
/**
 * Projection runner that manages the event processing loop.
 */
export declare class ProjectionRunner {
    private native;
    private projections;
    private initialized;
    constructor(native: ProjectionNativeBinding);
    /**
     * Initializes the projection storage.
     * Must be called before registering projections.
     */
    init(projectionDbPath: string): Promise<void>;
    /**
     * Registers a projection.
     * Creates the table if it doesn't exist.
     */
    register<TSchema extends SchemaDefinition>(name: string, options: ProjectionOptions<TSchema>): Promise<void>;
    /**
     * Starts processing events for all registered projections.
     */
    startAll(): Promise<void>;
    /**
     * Starts processing events for a specific projection.
     */
    private startProjection;
    /**
     * Processes a batch of events.
     */
    private processBatch;
    /**
     * Stops processing for all projections.
     */
    stopAll(): void;
    /**
     * Stops processing for a specific projection.
     */
    stop(name: string): void;
}
export {};
//# sourceMappingURL=runner.d.ts.map