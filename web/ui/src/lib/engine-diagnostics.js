// Reporting must remain available even while the engine worker is stuck.
export async function readEngineDiagnostics(game, timeoutMs = 250) {
  const methods = {
    dispatchPerf: 'lastDispatchPerf',
    snapshotPerf: 'lastSnapshotPerf',
    workCounters: 'lastWorkCounters',
    manaPaymentPerf: 'lastManaPaymentPerf',
    advanceUntilDecisionPerf: 'lastAdvanceUntilDecisionPerf',
  };
  const result = Object.fromEntries(Object.keys(methods).map(key => [key, null]));
  let expired = false;
  let timer;
  const reads = Promise.all(Object.entries(methods).map(async ([key, method]) => {
    try {
      const value = typeof game?.[method] === 'function' ? await game[method]() : null;
      if (!expired) result[key] = value;
    } catch { /* An older or failed worker may not expose this diagnostic. */ }
  }));
  try {
    await Promise.race([
      reads,
      new Promise(resolve => { timer = setTimeout(() => { expired = true; resolve(); }, timeoutMs); }),
    ]);
    return { ...result, timedOut: expired };
  } finally {
    clearTimeout(timer);
  }
}

export function runtimeEnvironmentDiagnostics(scope = globalThis) {
  const navigatorValue = scope?.navigator;
  const connection = navigatorValue?.connection || navigatorValue?.mozConnection || navigatorValue?.webkitConnection;
  const memory = scope?.performance?.memory;
  return {
    visibilityState: scope?.document?.visibilityState || null,
    online: typeof navigatorValue?.onLine === 'boolean' ? navigatorValue.onLine : null,
    userAgent: navigatorValue?.userAgent || null,
    hardwareConcurrency: navigatorValue?.hardwareConcurrency || null,
    deviceMemoryGb: navigatorValue?.deviceMemory || null,
    connection: connection ? {
      effectiveType: connection.effectiveType || null,
      downlinkMbps: Number.isFinite(connection.downlink) ? connection.downlink : null,
      rttMs: Number.isFinite(connection.rtt) ? connection.rtt : null,
      saveData: Boolean(connection.saveData),
    } : null,
    heap: memory ? {
      usedBytes: memory.usedJSHeapSize || null,
      totalBytes: memory.totalJSHeapSize || null,
      limitBytes: memory.jsHeapSizeLimit || null,
    } : null,
    screen: scope?.screen ? {
      width: scope.screen.width,
      height: scope.screen.height,
      pixelRatio: scope.devicePixelRatio || 1,
    } : null,
  };
}

export function diagnosticSignals(snapshot, engine) {
  const signals = [];
  const currentAgeMs = snapshot?.current ? Number(snapshot.at) - Number(snapshot.current.startedAt) : 0;
  const plannerNodes = Number(engine?.manaPaymentPerf?.visited_nodes ?? engine?.manaPaymentPerf?.visitedNodes);
  if (engine?.timedOut) signals.push('engine_worker_unresponsive');
  if (Number(snapshot?.engine?.queueWaitMs) >= 1000) signals.push('engine_queue_backlog');
  if (Number(snapshot?.engine?.wasmCallMs) >= 1000) signals.push('wasm_compute_slow');
  if (Number.isFinite(plannerNodes) && plannerNodes >= 1000) signals.push('mana_planner_search_large');
  if (engine?.manaPaymentPerf?.search_limited || engine?.manaPaymentPerf?.searchLimited) signals.push('mana_planner_limit_reached');
  if (Number(snapshot?.mainThread?.worstStallMs) >= 1000) signals.push('main_thread_stall');
  if (currentAgeMs >= 1000) signals.push('action_in_flight_slow');
  if ((snapshot?.peers || []).some(peer => peer.sinceReceivedMs >= 8000)) signals.push('peer_heartbeat_stale');
  if ((snapshot?.peers || []).some(peer => peer.rttMs >= 1000)) signals.push('peer_rtt_high');
  return signals;
}
