import { PUBLIC_FORMATS } from '../../ui/src/lib/relay/formats.js';

const ID = /^[a-f0-9]{32}$/;
const PEER = /^ws-([a-f0-9]{32})-([a-f0-9]{32})$/;
const TTL = 150_000;
const MAX_FRAME = 128 * 1024;
const json = (body, status = 200) => Response.json(body, { status });
const directory = (env) => env.DIRECTORY.get(env.DIRECTORY.idFromName('public'));
const room = (env, id) => env.ROOMS.get(env.ROOMS.idFromName(id));
const send = (ws, data) => { try { ws.send(JSON.stringify(data)); } catch { /* close event cleans up */ } };
const authority = config => ({
  currentHost: String(config?.currentHost || config?.host || ''),
  authorityEpoch: Number.isSafeInteger(Number(config?.authorityEpoch))
    ? Number(config.authorityEpoch) : 1,
});
const authorityError = (code, message) => Object.assign(new Error(message), { code });

export default {
  async fetch(request, env) {
    const origin = request.headers.get('Origin');
    const allowed = (env.ALLOWED_ORIGINS || '').split(',').map(x => x.trim());
    if (!origin || !allowed.includes(origin)) return json({ error: 'Origin not allowed' }, 403);
    const headers = { 'Access-Control-Allow-Origin': origin, 'Vary': 'Origin', 'Cache-Control': 'no-store' };
    if (request.method === 'OPTIONS') return new Response(null, { headers: { ...headers,
      'Access-Control-Allow-Methods': 'GET, OPTIONS' } });
    if (request.method !== 'GET') return json({ error: 'Method not allowed' }, 405);
    const url = new URL(request.url);
    let response;
    if (url.pathname === '/lobbies') response = await directory(env).fetch('https://internal/list');
    else {
      const match = url.pathname.match(/^\/rooms\/([a-f0-9]{32})\/socket$/);
      if (!match || request.headers.get('Upgrade')?.toLowerCase() !== 'websocket') return json({ error: 'Not found' }, 404);
      return room(env, match[1]).fetch(request);
    }
    return new Response(response.body, { status: response.status, headers: { ...Object.fromEntries(response.headers), ...headers } });
  }
};

// Only the Worker and Room binding can reach this object's internal routes.
export class LobbyDirectory {
  constructor(ctx) { this.ctx = ctx; }
  async fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/list') {
      const entries = await this.ctx.storage.list({ prefix: 'lobby:', limit: 200 });
      return json({ lobbies: [...entries.values()].filter(x => x.expiresAt > Date.now())
        .sort((a, b) => b.updatedAt - a.updatedAt) });
    }
    const listing = await request.json();
    if (!ID.test(listing.room)) return json({ error: 'Invalid room' }, 400);
    const key = `lobby:${listing.room}`;
    if (!listing.available) await this.ctx.storage.delete(key);
    else {
      const entries = await this.ctx.storage.list({ prefix: 'lobby:', limit: 200 });
      if (entries.size >= 200 && !entries.has(key)) return json({ error: 'Directory full' }, 503);
      await this.ctx.storage.put(key, { ...listing, expiresAt: Date.now() + TTL, updatedAt: Date.now() });
      if (!await this.ctx.storage.getAlarm()) await this.ctx.storage.setAlarm(Date.now() + TTL);
    }
    return json({ ok: true });
  }
  async alarm() {
    const entries = await this.ctx.storage.list({ prefix: 'lobby:' });
    const stale = [...entries].filter(([, value]) => value.expiresAt <= Date.now()).map(([key]) => key);
    if (stale.length) await this.ctx.storage.delete(stale);
    if (entries.size > stale.length) await this.ctx.storage.setAlarm(Date.now() + TTL);
  }
}

export class LobbyRoom {
  constructor(ctx, env) {
    this.ctx = ctx;
    this.env = env;
    ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair('ping', 'pong'));
  }
  sockets() { return this.ctx.getWebSockets().filter(ws => ws.readyState === 1); }
  async fetch(request) {
    const url = new URL(request.url);
    const roomId = url.pathname.split('/')[2];
    const peer = url.searchParams.get('peer');
    if (!PEER.test(peer || '') || peer.match(PEER)[1] !== roomId) return json({ error: 'Invalid peer' }, 400);
    if (this.sockets().length >= 8) return json({ error: 'Room full' }, 409);
    const pair = new WebSocketPair();
    this.ctx.acceptWebSocket(pair[1]);
    pair[1].serializeAttachment({ peer, room: roomId, authenticated: false, connectedAt: Date.now() });
    const alarm = await this.ctx.storage.getAlarm();
    if (!alarm || alarm > Date.now() + 30000) await this.ctx.storage.setAlarm(Date.now() + 30000);
    // Authentication is the first frame: credentials never appear in access URLs.
    return new Response(null, { status: 101, webSocket: pair[0] });
  }
  async webSocketMessage(ws, raw) {
    try {
      if (typeof raw !== 'string' || new TextEncoder().encode(raw).length > MAX_FRAME) throw new Error('Frame too large');
      const msg = JSON.parse(raw);
      let state = ws.deserializeAttachment();
      if (!state.authenticated) {
        if (msg.type !== 'auth' || !ID.test(msg.token || '')) throw new Error('Authentication required');
        // Serialize first-owner acquisition and reconnect credentials across awaits.
        const authError = await this.ctx.blockConcurrencyWhile(async () => {
          try {
            const existing = await this.ctx.storage.get(`peer:${state.peer}`);
            const config = await this.ctx.storage.get('config');
            if (msg.resume && (!existing || !config)) throw new Error('Saved lobby has expired');
            if (existing && existing !== msg.token) throw new Error('Identity already reserved');
            if (!config) {
              if (!Object.hasOwn(PUBLIC_FORMATS, msg.format)) throw new Error('A supported format is required');
              const rules = PUBLIC_FORMATS[msg.format];
              const desiredPlayers = rules.maxPlayers === 2 ? 2 : Number(msg.desiredPlayers);
              if (![2, 3, 4].includes(desiredPlayers)) throw new Error('Invalid player count');
              await this.ctx.storage.put('config', {
                host: state.peer, currentHost: state.peer, authorityEpoch: 1,
                format: msg.format, desiredPlayers,
              });
            } else if (!existing && !this.sockets().some(socket => {
              const a = socket.deserializeAttachment(); return a.authenticated && a.peer === authority(config).currentHost;
            })) throw new Error('Lobby host is offline');
            const peers = await this.ctx.storage.list({ prefix: 'peer:' });
            if (!existing && peers.size >= 16) throw new Error('Room identity limit reached');
            await this.ctx.storage.put(`peer:${state.peer}`, msg.token);
            await this.ctx.storage.delete('emptySince');
            for (const old of this.sockets()) {
              if (old !== ws && old.deserializeAttachment().peer === state.peer) old.close(4001, 'Reconnected');
            }
            state = { ...state, authenticated: true, messages: 0, window: Date.now() };
            ws.serializeAttachment(state);
            if (!await this.ctx.storage.getAlarm()) await this.ctx.storage.setAlarm(Date.now() + 24 * 60 * 60 * 1000);
          } catch (error) { return error.message; }
        });
        if (authError) throw new Error(authError);
        send(ws, { type: 'open', peer: state.peer, config: await this.ctx.storage.get('config') });
        return;
      }
      // Bound abuse and accidental loops without writing storage for game frames.
      if (Date.now() - state.window > 1000) { state.window = Date.now(); state.messages = 0; }
      if (++state.messages > 256) throw new Error('Rate limit exceeded');
      ws.serializeAttachment(state);
      if (msg.type === 'advertise') {
        const config = await this.ctx.storage.get('config');
        if (state.peer !== authority(config).currentHost) return;
        const listing = msg.lobby || {};
        const response = await directory(this.env).fetch('https://internal/update', { method: 'POST', body: JSON.stringify({
          room: state.room, id: authority(config).currentHost, format: config.format, securityMode: 'trusted',
          name: String(listing.name || 'Lobby').slice(0, 60), desiredPlayers: config.desiredPlayers,
          playerCount: Math.max(1, Math.min(config.desiredPlayers, Number(listing.playerCount) || 1)),
          available: listing.available === true && Number(listing.playerCount) < config.desiredPlayers,
        }) });
        if (!response.ok) send(ws, { type: 'error', message: 'Public directory is full; lobby is still joinable by code' });
        return;
      }
      // Authority controls are deliberately separate from gameplay frames;
      // future migration code can opt into fencing without changing relay
      // compatibility for existing offer/answer/data traffic.
      if (msg.type === 'authority_probe' || msg.type === 'authority_lease') {
        try {
          await this.handleAuthorityMessage(ws, state, msg);
        } catch (error) {
          send(ws, { type: 'authority_error', code: error.code || 'authority_rejected', message: error.message });
        }
        return;
      }
      if (!['offer', 'answer', 'data', 'close'].includes(msg.type) || !ID.test(msg.connectionId || '')) throw new Error('Invalid frame');
      const target = this.sockets().find(socket => {
        const a = socket.deserializeAttachment(); return a.authenticated && a.peer === msg.to;
      });
      if (!target) { send(ws, { type: 'unavailable', connectionId: msg.connectionId }); return; }
      // Ignore client-supplied sender identity; prohibit cross-room traffic by lookup.
      send(target, { type: msg.type, from: state.peer, connectionId: msg.connectionId,
        ...(msg.type === 'data' ? { data: msg.data } : {}),
        ...(['offer', 'answer'].includes(msg.type) ? { metadata: msg.metadata || {} } : {}) });
    } catch (error) {
      send(ws, { type: 'error', message: error.message });
      ws.close(1008, 'Invalid relay request');
    }
  }
  async handleAuthorityMessage(ws, state, msg) {
    let response = null;
    let targetSocket = null;
    let rejection = null;
    await this.ctx.blockConcurrencyWhile(async () => {
      const config = await this.ctx.storage.get('config');
      const current = authority(config);
      if (state.peer !== current.currentHost
          || !Number.isSafeInteger(Number(msg.authorityEpoch))
          || Number(msg.authorityEpoch) !== current.authorityEpoch) {
        rejection = authorityError('authority_fenced', 'Authority epoch is stale or not held by this peer');
        return;
      }
      if (msg.type === 'authority_probe') {
        response = { type: 'authority_probe_ack', currentHost: current.currentHost, authorityEpoch: current.authorityEpoch };
        return;
      }
      const nextHost = String(msg.nextHost || '');
      if (!PEER.test(nextHost) || nextHost.match(PEER)[1] !== state.room || nextHost === current.currentHost) {
        rejection = authorityError('authority_lease_rejected', 'Lease target is invalid');
        return;
      }
      targetSocket = this.sockets().find(socket => {
        const attachment = socket.deserializeAttachment();
        return attachment.authenticated && attachment.peer === nextHost;
      });
      if (!targetSocket) {
        rejection = authorityError('authority_lease_rejected', 'Lease target is not connected');
        return;
      }
      const nextEpoch = current.authorityEpoch + 1;
      await this.ctx.storage.put('config', {
        ...config, host: nextHost, currentHost: nextHost, authorityEpoch: nextEpoch,
      });
      response = {
        type: 'authority_lease_granted', previousHost: current.currentHost,
        currentHost: nextHost, authorityEpoch: nextEpoch,
      };
    });
    if (rejection) throw rejection;
    send(ws, response);
    if (response?.type === 'authority_lease_granted') send(targetSocket, {
      type: 'authority_lease', currentHost: response.currentHost, authorityEpoch: response.authorityEpoch,
    });
  }
  async webSocketClose(ws) {
    // Complete the close handshake explicitly, including in local runtimes.
    try { ws.close(1000, 'Connection closed'); } catch { /* already closed */ }
    const state = ws.deserializeAttachment();
    if (!state?.authenticated) return;
    if (this.sockets().some(s => s !== ws && s.deserializeAttachment().peer === state.peer)) return;
    for (const other of this.sockets()) send(other, { type: 'offline', peer: state.peer });
    const config = await this.ctx.storage.get('config');
    if (authority(config).currentHost === state.peer) await directory(this.env).fetch('https://internal/update', {
      method: 'POST', body: JSON.stringify({ room: state.room, available: false })
    });
  }
  async webSocketError(ws) { ws.close(1011, 'Relay error'); await this.webSocketClose(ws); }
  async alarm() {
    for (const socket of this.sockets()) {
      const attachment = socket.deserializeAttachment();
      if (!attachment.authenticated && Date.now() - attachment.connectedAt >= 15000) socket.close(1008, 'Authentication timed out');
    }
    if (this.sockets().some(socket => !socket.deserializeAttachment().authenticated)) {
      await this.ctx.storage.setAlarm(Date.now() + 30000); return;
    }
    if (this.sockets().length) await this.ctx.storage.setAlarm(Date.now() + 24 * 60 * 60 * 1000);
    else {
      const emptySince = await this.ctx.storage.get('emptySince') ?? Date.now();
      if (Date.now() - emptySince >= 24 * 60 * 60 * 1000) await this.ctx.storage.deleteAll();
      else {
        await this.ctx.storage.put('emptySince', emptySince);
        await this.ctx.storage.setAlarm(emptySince + 24 * 60 * 60 * 1000);
      }
    }
  }
}
