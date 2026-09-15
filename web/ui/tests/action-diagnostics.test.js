import test from "node:test";
import assert from "node:assert/strict";
import {
  beginActionTrace,
  completeActionTrace,
  currentActionTrace,
  getDiagnosticsSnapshot,
  markActionStage,
  recordMultiplayerEvent,
  recordMultiplayerState,
  recordPeerMessage,
  recordPeerRtt,
  resetDiagnostics,
  subscribeDiagnostics,
  tagActionTrace,
} from "../src/lib/action-diagnostics.js";

test("a trace records ordered stages with elapsed times and closes once", () => {
  resetDiagnostics();
  const id = beginActionTrace({ label: "pass", command: { type: "priority_action", action_index: 0 } });
  markActionStage(id, "sent", { peers: 2 });
  markActionStage(id, "applied");
  assert.equal(currentActionTrace()?.id, id);
  assert.equal(completeActionTrace(id), id);
  assert.equal(completeActionTrace(id), null, "second completion is ignored");
  const [trace] = getDiagnosticsSnapshot().traces;
  assert.equal(trace.done, true);
  assert.deepEqual(trace.stages.map((stage) => stage.name), ["sent", "applied", "done"]);
  assert.ok(trace.stages.every((stage, index, all) => index === 0 || stage.sinceStartMs >= all[index - 1].sinceStartMs));
  assert.equal(trace.stages[0].meta.peers, 2);
  assert.equal(currentActionTrace(), null);
});

test("later pipeline stages find the trace by request id or intent key", () => {
  resetDiagnostics();
  const first = beginActionTrace({ label: "first" });
  tagActionTrace(first, { requestId: "req-1" });
  const second = beginActionTrace({ label: "second" });
  assert.equal(getDiagnosticsSnapshot().traces.find((trace) => trace.id === first).outcome, "superseded", "a new action closes the previous open trace");
  tagActionTrace(second, { requestId: "req-2", actionIntentKey: "intent-2" });
  assert.equal(markActionStage({ requestId: "req-2" }, "ack"), second);
  assert.equal(markActionStage("intent-2", "echo"), second);
  assert.equal(markActionStage("missing-key", "x"), second, "unknown keys fall back to the newest open trace");
  assert.equal(markActionStage("req-1", "late-echo"), first, "keyed stages still reach a finished trace");
  completeActionTrace(second);
  assert.equal(markActionStage(null, "late"), null, "nothing open, nothing to attach to");
});

test("peer bookkeeping tracks round trips, message counts and quiet time", () => {
  resetDiagnostics();
  recordPeerRtt("peer-a", 120, "Alice");
  recordPeerRtt("peer-a", 80);
  recordPeerMessage("peer-a", "in", "action_intent", 512);
  recordPeerMessage("peer-a", "out", "peer_heartbeat", 40);
  const [peer] = getDiagnosticsSnapshot().peers;
  assert.equal(peer.name, "Alice");
  assert.equal(peer.rttMs, 80);
  assert.equal(peer.rttAvgMs, 100);
  assert.equal(peer.received, 1);
  assert.equal(peer.sent, 1);
  assert.equal(peer.bytesIn, 512);
  assert.ok(peer.sinceReceivedMs >= 0);
  const events = getDiagnosticsSnapshot().events;
  assert.equal(events.length, 1, "heartbeats do not clutter the event log");
  assert.equal(events[0].kind, "message:in");
});

test("subscribers are told about every change", () => {
  resetDiagnostics();
  let calls = 0;
  const unsubscribe = subscribeDiagnostics(() => { calls += 1; });
  const id = beginActionTrace({ label: "x" });
  markActionStage(id, "y");
  unsubscribe();
  completeActionTrace(id);
  assert.equal(calls, 2);
});

test("multiplayer diagnostics are bounded, ordered, and sanitized", () => {
  resetDiagnostics();
  for (let i = 0; i < 90; i++) recordMultiplayerEvent(`event-${i}`, { seq: i, prefixHash: "1234567890abcdefSECRET", token: "hidden" });
  const snapshot = getDiagnosticsSnapshot().multiplayer;
  assert.equal(snapshot.events.length, 80);
  assert.equal(snapshot.events[0].kind, "event-89");
  assert.equal(snapshot.events.at(-1).kind, "event-10");
  assert.equal(snapshot.events[0].prefixHash, "1234567890abcdef");
  assert.equal("token" in snapshot.events[0], false);
});

test("multiplayer snapshot exposes current state without changing action bookkeeping", () => {
  resetDiagnostics();
  recordMultiplayerState({ connected: true, role: "guest", seq: 4, expectedSeq: 5, paused: false, resyncPending: true, hostConnection: "open", pendingCommands: 2, acceptedActions: 4, peerId: "peer-1" });
  const snapshot = getDiagnosticsSnapshot();
  assert.deepEqual(snapshot.multiplayer.current, { connected: true, role: "guest", peerId: "peer-1", connectionState: null, seq: 4, expectedSeq: 5, prefixHash: null, paused: false, resyncPending: true, hostConnection: "open", pendingCommands: 2, acceptedActions: 4, reconnects: 0 });
  assert.equal(snapshot.traces.length, 0);
});
