import test from 'node:test';
import assert from 'node:assert/strict';
import {
  diagnosticSignals,
  readEngineDiagnostics,
  runtimeEnvironmentDiagnostics,
} from '../src/lib/engine-diagnostics.js';

test('exports available counters despite a stuck worker request', async () => {
  let finish;
  const report = await readEngineDiagnostics({
    lastDispatchPerf: () => ({ elapsedMs: 12 }),
    lastWorkCounters: () => new Promise(resolve => { finish = resolve; }),
    lastSnapshotPerf: () => Promise.reject(new Error('worker failed')),
  }, 10);
  assert.equal(report.timedOut, true);
  assert.deepEqual(report.dispatchPerf, { elapsedMs: 12 });
  assert.equal(report.workCounters, null);
  finish({ late: true });
  await Promise.resolve();
  assert.equal(report.workCounters, null);
});

test('supports missing diagnostics and successful reads without timeout', async () => {
  const report = await readEngineDiagnostics({ lastManaPaymentPerf: () => ({ visited_nodes: 3 }) });
  assert.equal(report.timedOut, false);
  assert.deepEqual(report.manaPaymentPerf, { visited_nodes: 3 });
  assert.equal(report.dispatchPerf, null);
});

test('classifies engine, planner, browser and peer bottlenecks independently', () => {
  const signals = diagnosticSignals({
    at: 5000,
    current: { startedAt: 3000 },
    engine: { queueWaitMs: 1200, wasmCallMs: 2400 },
    mainThread: { worstStallMs: 1500 },
    peers: [{ sinceReceivedMs: 9000, rttMs: 1200 }],
  }, {
    timedOut: false,
    manaPaymentPerf: { visited_nodes: 4096, search_limited: true },
  });
  assert.deepEqual(signals, [
    'engine_queue_backlog',
    'wasm_compute_slow',
    'mana_planner_search_large',
    'mana_planner_limit_reached',
    'main_thread_stall',
    'action_in_flight_slow',
    'peer_heartbeat_stale',
    'peer_rtt_high',
  ]);
});

test('captures available runtime environment without requiring browser-only APIs', () => {
  const report = runtimeEnvironmentDiagnostics({
    navigator: { onLine: true, userAgent: 'test-agent', hardwareConcurrency: 8 },
    document: { visibilityState: 'hidden' },
    performance: {},
  });
  assert.equal(report.online, true);
  assert.equal(report.userAgent, 'test-agent');
  assert.equal(report.hardwareConcurrency, 8);
  assert.equal(report.visibilityState, 'hidden');
  assert.equal(report.connection, null);
});
