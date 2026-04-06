#!/usr/bin/env node
/**
 * Memory Flash Bridge
 *
 * Connects to RuVector PostgreSQL, listens for `memory_access` NOTIFY events
 * (fired by the trigger on memory_entries), and POSTs each event to the
 * VisionFlow REST API so it gets broadcast to all WebSocket clients.
 *
 * Usage:
 *   node scripts/memory-flash-bridge.mjs [--visionflow-url http://localhost:3001]
 *
 * Environment:
 *   PGHOST / PGPORT / PGUSER / PGPASSWORD / PGDATABASE — RuVector PG connection
 *   VISIONFLOW_URL — base URL for VisionFlow API (default: http://localhost:3001)
 */

import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const pg = require('/usr/local/lib/node_modules/pg');

const { Client } = pg;

const args = process.argv.slice(2);
const VISIONFLOW_URL =
  args.find((_, i) => args[i - 1] === '--visionflow-url') ||
  process.env.VISIONFLOW_URL ||
  'http://localhost:3001';

const ENDPOINT = `${VISIONFLOW_URL}/api/memory-flash`;
const BATCH_ENDPOINT = `${VISIONFLOW_URL}/api/memory-flash/batch`;
const BATCH_WINDOW_MS = 100; // Batch events within 100ms window

let pending = [];
let batchTimer = null;

async function flushBatch() {
  if (pending.length === 0) return;
  const events = pending.splice(0);
  batchTimer = null;

  try {
    const res = await fetch(
      events.length === 1 ? ENDPOINT : BATCH_ENDPOINT,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(
          events.length === 1 ? events[0] : { events }
        ),
      }
    );
    if (!res.ok) {
      console.error(`[flash-bridge] POST failed: ${res.status} ${res.statusText}`);
    }
  } catch (err) {
    console.error(`[flash-bridge] POST error: ${err.message}`);
  }
}

function queueEvent(payload) {
  pending.push(payload);
  if (!batchTimer) {
    batchTimer = setTimeout(flushBatch, BATCH_WINDOW_MS);
  }
}

async function main() {
  const client = new Client({
    host: process.env.PGHOST || 'ruvector-postgres',
    port: parseInt(process.env.PGPORT || '5432'),
    user: process.env.PGUSER || 'ruvector',
    password: process.env.PGPASSWORD || 'ruvector',
    database: process.env.PGDATABASE || 'ruvector',
  });

  client.on('error', (err) => {
    console.error('[flash-bridge] PG client error:', err.message);
    process.exit(1);
  });

  await client.connect();
  console.log(`[flash-bridge] Connected to RuVector PG`);
  console.log(`[flash-bridge] Forwarding memory_access events to ${ENDPOINT}`);

  client.on('notification', (msg) => {
    if (msg.channel !== 'memory_access') return;
    try {
      const payload = JSON.parse(msg.payload);
      queueEvent({
        key: payload.key || '',
        namespace: payload.namespace || '',
        action: payload.action || 'access',
      });
    } catch (err) {
      console.error('[flash-bridge] Bad NOTIFY payload:', msg.payload);
    }
  });

  await client.query('LISTEN memory_access');
  console.log('[flash-bridge] Listening for memory_access notifications');

  // Keep alive
  const keepAlive = setInterval(async () => {
    try {
      await client.query('SELECT 1');
    } catch {
      console.error('[flash-bridge] Keep-alive failed, exiting');
      clearInterval(keepAlive);
      process.exit(1);
    }
  }, 30000);

  // Graceful shutdown
  for (const sig of ['SIGINT', 'SIGTERM']) {
    process.on(sig, async () => {
      console.log(`[flash-bridge] ${sig} received, shutting down`);
      clearInterval(keepAlive);
      await flushBatch();
      await client.end();
      process.exit(0);
    });
  }
}

main().catch((err) => {
  console.error('[flash-bridge] Fatal:', err);
  process.exit(1);
});
