#!/usr/bin/env node
/**
 * Batch embedding worker for RuVector PostgreSQL
 *
 * Embeds entries missing embeddings using Xenova/all-MiniLM-L6-v2 (384 dims)
 * Compatible with ruflo's recall pipeline.
 *
 * Usage: node scripts/batch-embed.js [--batch-size 100] [--limit 10000] [--source swarm]
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const pg = require('/usr/local/lib/node_modules/pg');

const { Pool } = pg;

const pool = new Pool({
  host: process.env.PGHOST || 'ruvector-postgres',
  port: parseInt(process.env.PGPORT || '5432'),
  user: process.env.PGUSER || 'ruvector',
  password: process.env.PGPASSWORD || 'ruvector',
  database: process.env.PGDATABASE || 'ruvector',
  max: 3,
});

const args = process.argv.slice(2);
const BATCH_SIZE = parseInt(args.find((_, i) => args[i-1] === '--batch-size') || '50');
const LIMIT = parseInt(args.find((_, i) => args[i-1] === '--limit') || '0');
const SOURCE = args.find((_, i) => args[i-1] === '--source') || null;

let embedder = null;

async function getEmbedder() {
  if (!embedder) {
    const { pipeline } = await import('@xenova/transformers');
    embedder = await pipeline('feature-extraction', 'Xenova/all-MiniLM-L6-v2');
    console.log('Embedder loaded: Xenova/all-MiniLM-L6-v2 (384 dims)');
  }
  return embedder;
}

async function embedText(text) {
  const pipe = await getEmbedder();
  // Truncate to ~256 tokens (~1024 chars) for speed
  const truncated = text.substring(0, 1024);
  const output = await pipe(truncated, { pooling: 'mean', normalize: true });
  return Array.from(output.data);
}

function extractText(value) {
  try {
    if (typeof value === 'string') {
      try {
        const parsed = JSON.parse(value);
        if (typeof parsed === 'string') return parsed;
        if (parsed.description) return parsed.description;
        if (parsed.output) return String(parsed.output).substring(0, 500);
        if (parsed.command) return `command: ${parsed.command}`;
        if (parsed.data) return String(parsed.data);
        return JSON.stringify(parsed).substring(0, 500);
      } catch {
        return value;
      }
    }
    if (typeof value === 'object') {
      if (value.description) return value.description;
      if (value.output) return String(value.output).substring(0, 500);
      return JSON.stringify(value).substring(0, 500);
    }
    return String(value);
  } catch {
    return '';
  }
}

async function processBatch() {
  const client = await pool.connect();

  try {
    // Fetch batch of entries without embeddings
    let query = `
      SELECT id, key, namespace, value::text
      FROM memory_entries
      WHERE embedding IS NULL
    `;
    if (SOURCE) query += ` AND source_type = '${SOURCE}'`;
    query += ` ORDER BY created_at DESC LIMIT ${BATCH_SIZE}`;

    const { rows } = await client.query(query);
    if (rows.length === 0) return 0;

    let embedded = 0;
    for (const row of rows) {
      try {
        const text = `${row.namespace}: ${row.key} ${extractText(row.value)}`;
        if (text.trim().length < 5) continue;

        const embedding = await embedText(text);
        const embeddingStr = `[${embedding.join(',')}]`;

        await client.query(
          `UPDATE memory_entries SET embedding = $1::ruvector, embedding_json = $2::jsonb WHERE id = $3`,
          [embeddingStr, JSON.stringify(embedding), row.id]
        );
        embedded++;
      } catch (e) {
        // Skip entries that fail embedding
      }
    }
    return embedded;
  } finally {
    client.release();
  }
}

async function main() {
  console.log(`Batch embedder starting (batch_size=${BATCH_SIZE}, limit=${LIMIT || 'unlimited'}, source=${SOURCE || 'all'})`);

  // Get initial count
  const { rows: [{ count: remaining }] } = await pool.query(
    `SELECT count(id) as count FROM memory_entries WHERE embedding IS NULL` +
    (SOURCE ? ` AND source_type = '${SOURCE}'` : '')
  );
  console.log(`${remaining} entries need embedding`);

  let totalEmbedded = 0;
  const startTime = Date.now();

  while (true) {
    const count = await processBatch();
    if (count === 0) break;
    totalEmbedded += count;

    const elapsed = (Date.now() - startTime) / 1000;
    const rate = totalEmbedded / elapsed;
    const remainingTime = ((remaining - totalEmbedded) / rate / 3600).toFixed(1);

    process.stdout.write(`\r  Embedded: ${totalEmbedded} (${rate.toFixed(1)}/s, ~${remainingTime}h remaining)  `);

    if (LIMIT > 0 && totalEmbedded >= LIMIT) {
      console.log(`\nReached limit of ${LIMIT}`);
      break;
    }
  }

  console.log(`\nDone: ${totalEmbedded} entries embedded in ${((Date.now() - startTime) / 1000).toFixed(0)}s`);
  await pool.end();
}

main().catch(e => { console.error(e); process.exit(1); });
