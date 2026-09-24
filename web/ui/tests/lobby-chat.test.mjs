import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

// Exercise the messaging callbacks with the same session/transport boundaries
// used by the hook, without starting an engine or signaling server.
const source = readFileSync(new URL("../src/hooks/peer-lobby/messaging.js", import.meta.url), "utf8");
const callbacks = "const MAX_LOBBY_CHAT_LENGTH = 240;\n" + source.slice(source.indexOf("  const receiveLobbyChat ="), source.indexOf("  const handleHostMessage ="));
function harness(role = "host") {
  const multiplayerRef = { current: { role, localPeerId: "a", players: [
    { peerId: "a", name: "Alice" }, { peerId: "b", currentPeerId: "b-current", name: "Bob" },
  ], chatMessages: [] } };
  const sent = [];
  const clientConnection = { open: true, peer: "b" };
  const api = new Function("useCallback", "multiplayerRef", "hostConnectionRef", "peerConnectionsRef", "clientConnectionsRef", "updateMultiplayer",
    "broadcastToClients", "safeSend", "PROTOCOL_VERSION", callbacks + "\nreturn { sendLobbyChat, publishLobbyChat, receiveLobbyChat, broadcastLobbyChat };")(
    (fn) => fn, multiplayerRef, { current: {} }, { current: new Map() },
    { current: new Map([["b", clientConnection]]) },
    (update) => { multiplayerRef.current = update(multiplayerRef.current); },
    (message) => sent.push(message), (_, message) => { sent.push(message); return true; }, 1);
  return { ...api, sent, multiplayerRef };
}
test("host attributes remote messages to their connection and broadcasts once", () => {
  const h = harness();
  assert.equal(h.publishLobbyChat("b-current", " hello "), true);
  assert.equal(h.sent[0].entry.name, "Bob");
  assert.equal(h.sent[0].entry.text, "hello");
  assert.equal(h.publishLobbyChat("stranger", "spoof"), false);
  h.receiveLobbyChat(h.sent[0].entry);
  assert.equal(h.multiplayerRef.current.chatMessages.length, 1);
});
test("chat rejects empty/oversized messages and bounds retained history", () => {
  const h = harness();
  assert.equal(h.sendLobbyChat(" "), false);
  assert.equal(h.sendLobbyChat("x".repeat(501)), false);
  for (let i = 0; i < 110; i++) h.sendLobbyChat(String(i));
  assert.equal(h.multiplayerRef.current.chatMessages.length, 100);
  assert.equal(h.multiplayerRef.current.chatMessages[0].text, "10");
});
test("clients send to host and wait for canonical echo", () => {
  const h = harness("client");
  assert.equal(h.sendLobbyChat("hello"), true);
  assert.equal(h.sent[0].type, "lobby_chat_send");
  assert.equal(h.multiplayerRef.current.chatMessages.length, 0);
  h.receiveLobbyChat({ id: "1", name: "Alice", text: "hello" });
  assert.equal(h.multiplayerRef.current.chatMessages.length, 1);
});

test("clients fall back to a direct host connection during a match", () => {
  const h = harness("client");
  h.multiplayerRef.current.hostPeerId = "host";
  const direct = { open: true, peer: "host" };
  const sent = [];
  const api = new Function("useCallback", "multiplayerRef", "hostConnectionRef", "peerConnectionsRef", "clientConnectionsRef", "updateMultiplayer", "broadcastToClients", "safeSend", "PROTOCOL_VERSION", callbacks + "\nreturn { sendLobbyChat };")(
    (fn) => fn, h.multiplayerRef, { current: { open: false } }, { current: new Map([["host", direct]]) },
    { current: new Map() }, (update) => { h.multiplayerRef.current = update(h.multiplayerRef.current); },
    () => {}, (_, message) => { sent.push(message); return true; }, 1);
  assert.equal(api.sendLobbyChat("hello from direct"), true);
  assert.equal(sent[0].type, "lobby_chat_send");
});
