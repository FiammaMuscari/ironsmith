import { PeerServer } from "peer";

function readEnv(name, fallback = "") {
  const value = process.env[name];
  if (typeof value !== "string") return fallback;
  const trimmed = value.trim();
  return trimmed || fallback;
}

function parseNumber(value, fallback) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function parseBoolean(value, fallback) {
  if (!value) return fallback;
  const normalized = value.trim().toLowerCase();
  if (["1", "true", "yes", "on"].includes(normalized)) return true;
  if (["0", "false", "no", "off"].includes(normalized)) return false;
  return fallback;
}

function ensurePath(path) {
  const trimmed = String(path || "/peerjs").trim() || "/peerjs";
  if (trimmed === "/") return trimmed;
  const withLeadingSlash = trimmed.startsWith("/") ? trimmed : `/${trimmed}`;
  return withLeadingSlash.endsWith("/")
    ? withLeadingSlash.slice(0, -1)
    : withLeadingSlash;
}

const host = readEnv("PEER_SERVER_HOST", "0.0.0.0");
const port = parseNumber(readEnv("PEER_SERVER_PORT", "9000"), 9000);
const path = ensurePath(readEnv("PEER_SERVER_PATH", "/peerjs"));
const key = readEnv("PEER_SERVER_KEY", "peerjs");
const allowDiscovery = parseBoolean(
  readEnv("PEER_SERVER_ALLOW_DISCOVERY", ""),
  false
);
const proxiedRaw = readEnv("PEER_SERVER_PROXIED", "");
const corsRaw = readEnv("PEER_SERVER_CORS", "");

const options = {
  host,
  port,
  path,
  key,
  allow_discovery: allowDiscovery,
};

if (proxiedRaw) {
  const proxied = parseBoolean(proxiedRaw, null);
  options.proxied = proxied === null ? proxiedRaw : proxied;
}

if (corsRaw) {
  const origins = corsRaw
    .split(",")
    .map((origin) => origin.trim())
    .filter(Boolean);
  if (origins.length > 0) {
    options.corsOptions = {
      origin: origins,
      credentials: true,
    };
  }
}

const server = PeerServer(options, () => {
  console.log("");
  console.log("PeerServer ready");
  console.log(`  bind: ${host}:${port}`);
  console.log(`  path: ${path}`);
  console.log(`  key: ${key}`);
  console.log("  browser config:");
  console.log(`    VITE_PEER_HOST=${host === "0.0.0.0" ? "<this-machine-lan-ip>" : host}`);
  console.log(`    VITE_PEER_PORT=${port}`);
  console.log(`    VITE_PEER_PATH=${path}`);
  console.log(`    VITE_PEER_KEY=${key}`);
  console.log("    VITE_PEER_SECURE=false");
  console.log("");
});

server.on("connection", (client) => {
  console.log(`peer connected: ${client.getId()}`);
});

server.on("disconnect", (client) => {
  console.log(`peer disconnected: ${client.getId()}`);
});

server.on("error", (err) => {
  console.error("PeerServer error:", err);
});
