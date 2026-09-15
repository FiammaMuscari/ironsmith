import test from 'node:test';
import assert from 'node:assert/strict';
import { startRelay } from './runtime.mjs';
import { randomBytes } from 'node:crypto';
const id = () => randomBytes(16).toString('hex');
const origin = 'http://localhost:5173';
function inbox(socket) {
  const queue = []; const waits = [];
  socket.addEventListener('message', event => { const value = JSON.parse(event.data); const waiter = waits.shift(); if (waiter) waiter(value); else queue.push(value); });
  return () => queue.length ? Promise.resolve(queue.shift()) : new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Timed out waiting for relay')), 5000);
    waits.push(value => { clearTimeout(timer); resolve(value); });
  });
}
test('Durable Object authenticates peers, relays only within rooms, lists and removes advertisements', { timeout: 30000 }, async t => {
  const mf = await startRelay(); t.after(() => mf.dispose());
  const room = id(); const host = `ws-${room}-${id()}`; const guest = `ws-${room}-${id()}`;
  async function connect(peer, token, format) {
    const response = await mf.dispatchFetch(`http://localhost/rooms/${room}/socket?peer=${peer}`, { headers: { Origin: origin, Upgrade: 'websocket' } });
    assert.equal(response.status, 101);
    const socket = response.webSocket; socket.accept(); const next = inbox(socket);
    socket.send(JSON.stringify({ type: 'auth', token, format, desiredPlayers: 2 }));
    return { socket, next, first: await next() };
  }
  assert.equal((await mf.dispatchFetch('http://localhost/lobbies', { headers: { Origin: 'https://evil.example' } })).status, 403);
  const token = id(); const h = await connect(host, token, 'modern'); assert.equal(h.first.type, 'open');
  const g = await connect(guest, id()); assert.equal(g.first.type, 'open');
  h.socket.send(JSON.stringify({ type: 'advertise', lobby: { name: 'Modern table', format: 'vintage', available: true, playerCount: 1 } }));
  let listing;
  for (let i = 0; i < 30; i++) {
    listing = await (await mf.dispatchFetch('http://localhost/lobbies', { headers: { Origin: origin } })).json();
    if (listing.lobbies.length) break;
    await new Promise(r => setTimeout(r, 30));
  }
  assert.equal(listing.lobbies[0].format, 'modern'); assert.equal(listing.lobbies[0].id, host);
  const connectionId = id();
  g.socket.send(JSON.stringify({ type: 'data', to: host, from: 'spoofed', connectionId, data: '.{"value":42}' }));
  assert.deepEqual(await h.next(), { type: 'data', from: guest, connectionId, data: '.{"value":42}' });
  g.socket.send(JSON.stringify({ type: 'data', to: `ws-${id()}-${id()}`, connectionId, data: '.' }));
  assert.equal((await g.next()).type, 'unavailable');
  const thief = await connect(host, id()); assert.equal(thief.first.type, 'error');
  const reconnect = await connect(host, token); assert.equal(reconnect.first.type, 'open');
  reconnect.socket.close(1000, 'leave');
  for (let i = 0; i < 30; i++) {
    listing = await (await mf.dispatchFetch('http://localhost/lobbies', { headers: { Origin: origin } })).json();
    if (!listing.lobbies.length) break;
    await new Promise(r => setTimeout(r, 30));
  }
  assert.equal(listing.lobbies.length, 0);
});

test('an expired resume credential cannot create a replacement room', async t => {
  const mf = await startRelay(); t.after(() => mf.dispose());
  const room = id(), peer = `ws-${room}-${id()}`;
  const response = await mf.dispatchFetch(`http://localhost/rooms/${room}/socket?peer=${peer}`, { headers: { Origin: origin, Upgrade: 'websocket' } });
  const socket = response.webSocket; socket.accept(); const next = inbox(socket);
  socket.send(JSON.stringify({ type: 'auth', token: id(), resume: true, format: 'modern', desiredPlayers: 2 }));
  assert.match((await next()).message, /expired/);
});

test('authority epochs fence stale peers and preserve the normal host flow', { timeout: 30000 }, async t => {
  const mf = await startRelay(); t.after(() => mf.dispose());
  const room = id(); const host = `ws-${room}-${id()}`; const guest = `ws-${room}-${id()}`;
  async function connect(peer, token, format) {
    const response = await mf.dispatchFetch(`http://localhost/rooms/${room}/socket?peer=${peer}`, { headers: { Origin: origin, Upgrade: 'websocket' } });
    assert.equal(response.status, 101);
    const socket = response.webSocket; socket.accept(); const next = inbox(socket);
    socket.send(JSON.stringify({ type: 'auth', token, format, desiredPlayers: 2 }));
    return { socket, next, first: await next() };
  }
  const hostToken = id();
  const a = await connect(host, hostToken, 'modern');
  const b = await connect(guest, id());
  assert.equal(a.first.type, 'open');
  assert.equal(a.first.config.currentHost, host);
  assert.equal(a.first.config.authorityEpoch, 1);

  a.socket.send(JSON.stringify({ type: 'authority_probe', authorityEpoch: 1 }));
  assert.deepEqual(await a.next(), { type: 'authority_probe_ack', currentHost: host, authorityEpoch: 1 });

  a.socket.send(JSON.stringify({ type: 'authority_lease', authorityEpoch: 1, nextHost: guest }));
  assert.deepEqual(await a.next(), { type: 'authority_lease_granted', previousHost: host, currentHost: guest, authorityEpoch: 2 });
  assert.deepEqual(await b.next(), { type: 'authority_lease', currentHost: guest, authorityEpoch: 2 });

  a.socket.send(JSON.stringify({ type: 'authority_probe', authorityEpoch: 1 }));
  assert.equal((await a.next()).code, 'authority_fenced');
  b.socket.send(JSON.stringify({ type: 'authority_probe', authorityEpoch: 2 }));
  assert.deepEqual(await b.next(), { type: 'authority_probe_ack', currentHost: guest, authorityEpoch: 2 });
  a.socket.send(JSON.stringify({ type: 'authority_probe', authorityEpoch: 999 }));
  assert.equal((await a.next()).code, 'authority_fenced');
  a.socket.send(JSON.stringify({ type: 'authority_lease', authorityEpoch: 1, nextHost: guest }));
  assert.equal((await a.next()).code, 'authority_fenced');

  a.socket.close(1000, 'reconnect');
  const resumed = await connect(host, hostToken);
  assert.equal(resumed.first.type, 'open');
  assert.equal(resumed.first.config.currentHost, guest);
  assert.equal(resumed.first.config.authorityEpoch, 2);
  resumed.socket.send(JSON.stringify({ type: 'authority_probe', authorityEpoch: 1 }));
  assert.equal((await resumed.next()).code, 'authority_fenced');
  b.socket.close(1000, 'leave'); resumed.socket.close(1000, 'leave');
});
