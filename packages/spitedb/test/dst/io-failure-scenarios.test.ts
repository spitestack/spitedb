/**
 * DST I/O Failure Scenarios
 *
 * Tests system behavior under various I/O failure conditions:
 * - Sync failures
 * - Read failures
 * - Write failures
 * - Partial writes (corruption)
 * - Intermittent failures
 *
 * Run with specific seed: SEED=12345 bun test test/dst/io-failure-scenarios.test.ts
 */

import { describe, test, beforeEach, afterEach, expect } from 'bun:test';
import {
  createTestEnvironment,
  createTestEventStore,
  createTestCoordinator,
  cleanupTestEnvironment,
  runWithSeedReporting,
  advanceTimeAsync,
  type TestEnvironment,
} from '../setup/test-helpers';
import type { EventStore } from '../../src/application/event-store';
import type { ProjectionCoordinator } from '../../src/application/projections';
import { getSeedFromEnv } from '../setup/seeded-random';
import { FaultScheduler, createFaultScheduler } from '../setup/fault-scheduler';
import { createMockAggregatorRegistration, MockAggregatorProjection } from '../setup/mock-projection';
import { generateRandomEvents, generateRandomWorkload } from '../setup/test-fixtures';
import {
  EventStoreInvariants,
  checkInvariants,
  runIOFailureScenario,
  type IOFailureScenario,
} from '../setup/dst-scenarios';
import { createDSTContext } from '../setup/test-helpers';

describe('DST: I/O Failure Scenarios', () => {
  let env: TestEnvironment;

  beforeEach(() => {
    env = createTestEnvironment(getSeedFromEnv());
  });

  afterEach(async () => {
    await cleanupTestEnvironment(env);
  });

  // ============================================================
  // Sync Failure Tests
  // ============================================================

  describe('Sync Failures', () => {
    test('syncFails on event flush throws error', async () => {
      await runWithSeedReporting(env, 'syncFails on flush', async () => {
        const store = await createTestEventStore(env);

        // Append events
        const events = generateRandomEvents(env.random, 5);
        await store.append('test-stream', events);

        // Inject sync failure
        env.fs.injectFault({ syncFails: true });

        // Flush should throw
        let flushFailed = false;
        try {
          await store.flush();
        } catch (e) {
          flushFailed = true;
          expect((e as Error).message).toContain('Simulated sync failure');
        }

        expect(flushFailed).toBe(true);
        env.fs.clearFaults();

        await store.close();
      });
    });

    test('syncFails on checkpoint preserves projection state', async () => {
      await runWithSeedReporting(env, 'syncFails on checkpoint', async () => {
        const store = await createTestEventStore(env);

        // Add events
        const events = generateRandomEvents(env.random, 10);
        await store.append('test-stream', events);
        await store.flush();

        // Create coordinator
        const coordinator = createTestCoordinator(env, store);
        coordinator.getRegistry().register(createMockAggregatorRegistration('counter', ['*']));

        await coordinator.start();
        // Advance time to allow projection polling to process events
        await advanceTimeAsync(env, 100);
        await coordinator.waitForCatchUp(1000);

        // Inject sync failure
        env.fs.injectFault({ syncFails: true });

        // Checkpoint should fail but projection continues
        try {
          await coordinator.forceCheckpoint();
        } catch {
          // Expected
        }

        env.fs.clearFaults();

        // Projection should still be functional
        const projection = coordinator.getProjection('counter') as MockAggregatorProjection;
        expect(projection?.getState()).toBe(10);

        // Retry checkpoint should work
        await coordinator.forceCheckpoint();

        await coordinator.stop();
        await store.close();
      });
    });

    test('sync delay simulation works with clock', async () => {
      await runWithSeedReporting(env, 'sync delay', async () => {
        const handle = await env.fs.open('/test-sync-delay', 'write');
        const startTime = env.clock.now();

        // Inject sync delay
        env.fs.injectFault({ syncDelayMs: 100 });

        // Start sync (don't await yet - it's waiting for clock.sleep)
        const syncPromise = env.fs.sync(handle);

        // Yield until the sync delay registers on the simulated clock.
        for (let i = 0; i < 5 && env.clock.getPendingSleepCount() === 0; i += 1) {
          await Promise.resolve();
        }
        expect(env.clock.getPendingSleepCount()).toBeGreaterThan(0);

        await env.clock.tickAsync(100);
        await syncPromise;

        // Verify time advanced
        expect(env.clock.now()).toBe(startTime + 100);

        env.fs.clearFaults();
        await env.fs.close(handle);
      });
    });
  });

  // ============================================================
  // Read Failure Tests
  // ============================================================

  describe('Read Failures', () => {
    test('readFails on readStream - system handles gracefully', async () => {
      await runWithSeedReporting(env, 'readFails on readStream', async () => {
        const store = await createTestEventStore(env);

        // Add and flush events
        const events = generateRandomEvents(env.random, 5);
        await store.append('test-stream', events);
        await store.flush();

        // Close
        await store.close();

        // Inject read failure before reopening
        env.fs.injectFault({ readFails: true });

        // System may handle gracefully by skipping invalid segments
        // or may throw - both are acceptable
        let opened = false;
        try {
          const reopened = await createTestEventStore(env);
          opened = true;
          await reopened.close();
        } catch {
          // Read failure during open is acceptable
        }

        env.fs.clearFaults();

        // Should work without fault
        const recoveredStore = await createTestEventStore(env);
        const recovered = await recoveredStore.readStream('test-stream');
        // May have lost events if segments were skipped
        expect(recovered.length).toBeLessThanOrEqual(5);

        await recoveredStore.close();
      });
    });

    test('readFails on readGlobal propagates error', async () => {
      await runWithSeedReporting(env, 'readFails on readGlobal', async () => {
        const store = await createTestEventStore(env);

        // Add and flush events
        const events = generateRandomEvents(env.random, 5);
        await store.append('test-stream', events);
        await store.flush();

        // Inject read failure
        env.fs.injectFault({ readFails: true });

        // readGlobal should throw
        await expect(store.readGlobal(0)).rejects.toThrow('Simulated read failure');

        env.fs.clearFaults();

        // Should work without fault
        const globalEvents = await store.readGlobal(0);
        expect(globalEvents).toHaveLength(5);

        await store.close();
      });
    });

    test('readFails on checkpoint load throws error and recovery works', async () => {
      await runWithSeedReporting(env, 'readFails on checkpoint load', async () => {
        const store = await createTestEventStore(env);

        // Add events and process with projection
        const events = generateRandomEvents(env.random, 10);
        await store.append('test-stream', events);
        await store.flush();

        // First run - create checkpoint
        const coordinator1 = createTestCoordinator(env, store);
        coordinator1.getRegistry().register(createMockAggregatorRegistration('counter', ['*']));
        await coordinator1.start();
        // Advance time to allow projection polling to process events
        await advanceTimeAsync(env, 100);
        await coordinator1.waitForCatchUp(1000);
        await coordinator1.forceCheckpoint();
        await coordinator1.stop();
        await store.close();

        // Inject read failure
        env.fs.injectFault({ readFails: true });

        // Second run - checkpoint load fails with error
        // Need fresh store since fault affects all reads
        let store2Failed = false;
        let store2: EventStore | undefined;
        let coordinator2: ProjectionCoordinator | undefined;
        try {
          store2 = await createTestEventStore(env);
          coordinator2 = createTestCoordinator(env, store2);
          coordinator2.getRegistry().register(createMockAggregatorRegistration('counter', ['*']));
          await coordinator2.start();
        } catch (e) {
          store2Failed = true;
          // Either store or coordinator start can fail
          expect((e as Error).message).toContain('Simulated read failure');
        } finally {
          // Clean up resources even on failure to release locks
          if (coordinator2) {
            try {
              await coordinator2.stop();
            } catch {
              // Ignore stop errors during cleanup
            }
          }
          if (store2) {
            try {
              await store2.close();
            } catch {
              // Ignore close errors during cleanup
            }
          }
        }
        expect(store2Failed).toBe(true);

        env.fs.clearFaults();

        // Third run - without fault, should start fresh and process all events
        const store3 = await createTestEventStore(env);
        const coordinator3 = createTestCoordinator(env, store3);
        coordinator3.getRegistry().register(createMockAggregatorRegistration('counter', ['*']));
        await coordinator3.start();

        // Advance time to allow projection polling to process events
        await advanceTimeAsync(env, 100);
        await coordinator3.waitForCatchUp(1000);

        // Projection processes all events
        const projection = coordinator3.getProjection('counter') as MockAggregatorProjection;
        expect(projection?.getState()).toBe(10);

        await coordinator3.stop();
        await store3.close();
      });
    });
  });

  // ============================================================
  // Write Failure Tests
  // ============================================================

  describe('Write Failures', () => {
    test('writeFails during flush throws error', async () => {
      await runWithSeedReporting(env, 'writeFails during flush', async () => {
        const store = await createTestEventStore(env);

        const events = generateRandomEvents(env.random, 5);
        await store.append('test-stream', events);

        // Inject write failure before flush
        env.fs.injectFault({ writeFails: true });

        // Flush will fail due to write failure
        let flushFailed = false;
        try {
          await store.flush();
        } catch (e) {
          flushFailed = true;
          expect((e as Error).message).toContain('Simulated');
        }

        expect(flushFailed).toBe(true);
        env.fs.clearFaults();

        await store.close();
      });
    });

    test('partialWrite corruption detected via CRC32', async () => {
      await runWithSeedReporting(env, 'partialWrite CRC detection', async () => {
        const store = await createTestEventStore(env);

        // Add events
        const events = generateRandomEvents(env.random, 5);
        await store.append('test-stream', events);

        // Inject partial write
        env.fs.injectFault({ partialWrite: true });

        // Flush will result in corrupt data
        await store.flush();

        await store.close();

        env.fs.clearFaults();

        // Reopen should detect corruption and handle gracefully
        // (either recover what's valid or start fresh)
        const recoveredStore = await createTestEventStore(env);

        // May have fewer events due to partial write
        const recovered = await recoveredStore.readStream('test-stream');
        expect(recovered.length).toBeLessThanOrEqual(5);

        // Whatever we have should be valid
        if (recovered.length > 0) {
          checkInvariants(recovered, [EventStoreInvariants.sequentialRevisions]);
        }

        await recoveredStore.close();
      });
    });
  });

  // ============================================================
  // Intermittent Failure Tests
  // ============================================================

  describe('Intermittent Failures', () => {
    test('flaky sync eventually recovers', async () => {
      await runWithSeedReporting(env, 'flaky sync recovery', async () => {
        const store = await createTestEventStore(env);
        const scheduler = createFaultScheduler(env.random, env.fs);

        const events = generateRandomEvents(env.random, 5);
        await store.append('test-stream', events);

        // Try to flush with intermittent failures
        let flushed = false;
        for (let attempt = 0; attempt < 10; attempt++) {
          // 30% chance of sync failure each attempt
          if (env.random.bool(0.3)) {
            scheduler.injectFault('sync');
          }

          try {
            await store.flush();
            flushed = true;
            scheduler.clearFaults();
            break;
          } catch {
            scheduler.clearFaults();
            // Retry
          }
        }

        // Should eventually succeed
        expect(flushed).toBe(true);

        await store.close();
      });
    });

    test('operations survive random 10% failure rate', async () => {
      await runWithSeedReporting(env, 'survive 10% failure rate', async () => {
        const store = await createTestEventStore(env);
        const scheduler = createFaultScheduler(env.random, env.fs);

        const workload = generateRandomWorkload(env.random, 20, {
          appendProbability: 0.8,
          streamCount: 3,
          eventsPerAppend: { min: 1, max: 3 },
        });

        let successfulAppends = 0;

        for (const op of workload) {
          // 10% chance of fault
          const faulted = scheduler.maybeInjectFault(0.1);

          try {
            if (op.type === 'append' && op.streamId && op.events) {
              await store.append(op.streamId, op.events);
              await store.flush();
              if (!faulted) {
                successfulAppends++;
              }
            } else if (op.type === 'read' && op.streamId) {
              await store.readStream(op.streamId);
            } else if (op.type === 'readGlobal') {
              await store.readGlobal(op.fromPosition ?? 0);
            }
          } catch {
            // Expected under fault injection
          }

          scheduler.clearFaults();
        }

        // Verify invariants after workload
        const allEvents = await store.readGlobal(0);
        if (allEvents.length > 0) {
          checkInvariants(allEvents, [
            EventStoreInvariants.monotonicPositions,
            EventStoreInvariants.sequentialRevisions,
          ]);
        }

        await store.close();
      });
    });

    test('projection handles intermittent event store failures', async () => {
      await runWithSeedReporting(env, 'projection intermittent failures', async () => {
        const store = await createTestEventStore(env);

        // Add events
        const events = generateRandomEvents(env.random, 20);
        await store.append('test-stream', events);
        await store.flush();

        const scheduler = createFaultScheduler(env.random, env.fs);

        // Start coordinator
        const coordinator = createTestCoordinator(env, store);
        coordinator.getRegistry().register(createMockAggregatorRegistration('counter', ['*']));

        // Configure intermittent read failures
        scheduler.startIntermittentFailures({
          failProbability: 0.2,
          recoveryProbability: 0.5,
          faultTypes: ['read'],
        });

        await coordinator.start();

        // Wait with intermittent failures
        for (let i = 0; i < 10; i++) {
          scheduler.tickIntermittent();
          await env.clock.tickAsync(100);
        }

        scheduler.stopIntermittentFailures();

        // Eventually should catch up
        await coordinator.waitForCatchUp(5000);

        const projection = coordinator.getProjection('counter') as MockAggregatorProjection;
        expect(projection?.getState()).toBe(20);

        await coordinator.stop();
        await store.close();
      });
    });
  });

  // ============================================================
  // Scenario Runner Tests
  // ============================================================

  describe('Using IOFailureScenario Runner', () => {
    test('scenario: sync failure during append', async () => {
      await runWithSeedReporting(env, 'scenario sync failure', async () => {
        const ctx = createDSTContext(env.seed);
        const store = await createTestEventStore(ctx);

        const scenario: IOFailureScenario = {
          name: 'sync_failure_append',
          fault: { syncFails: true },
          operation: async () => {
            await store.append('test', [{ type: 'Test', data: {} }]);
            await store.flush();
          },
          expected: 'error',
        };

        const result = await runIOFailureScenario(ctx, scenario);
        expect(result.success).toBe(true);

        await store.close();
      });
    });
  });
});
