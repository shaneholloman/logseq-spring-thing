#!/usr/bin/env node
/**
 * Targeted embedding for actionable entries only:
 * 1. hooks: only bash:*:pre entries with real CLI commands (git, docker, npm, etc.)
 * 2. swarm-recovered: agent-assignments and file-history
 * Skips: bash-history (pointer stubs), metrics (numeric aggregates)
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
  const truncated = text.substring(0, 1024);
  const output = await pipe(truncated, { pooling: 'mean', normalize: true });
  return Array.from(output.data);
}

function extractText(value) {
  try {
    if (typeof value === 'string') {
      try { value = JSON.parse(value); } catch { return value; }
    }
    if (value.command) return `command: ${value.command}`;
    if (value.file) return `file: ${value.file} ${value.operation || ''}`;
    if (value.description) return value.description;
    if (value.data) return String(value.data);
    return JSON.stringify(value).substring(0, 500);
  } catch { return ''; }
}

async function processBatch(query, label) {
  const client = await pool.connect();
  try {
    const { rows } = await client.query(query);
    if (rows.length === 0) return 0;

    let embedded = 0;
    for (const row of rows) {
      try {
        const text = `${row.namespace}: ${row.key} ${extractText(row.value)}`;
        if (text.trim().length < 10) continue;
        const embedding = await embedText(text);
        const embeddingStr = `[${embedding.join(',')}]`;
        await client.query(
          `UPDATE memory_entries SET embedding = $1::ruvector, embedding_json = $2::jsonb WHERE id = $3`,
          [embeddingStr, JSON.stringify(embedding), row.id]
        );
        embedded++;
      } catch {}
    }
    return embedded;
  } finally {
    client.release();
  }
}

async function main() {
  // Phase 1: Actionable hooks — bash pre-hooks with real CLI commands
  console.log('=== Phase 1: Actionable CLI hooks ===');
  let totalEmbedded = 0;
  const startTime = Date.now();

  const hooksCountResult = await pool.query(`
    SELECT count(id) as cnt FROM memory_entries
    WHERE source_type = 'hooks' AND embedding IS NULL
    AND key LIKE 'bash:%:pre'
    AND (value->>'command' LIKE 'mkdir%' OR value->>'command' LIKE 'cd %' OR value->>'command' LIKE 'git %'
         OR value->>'command' LIKE 'npm %' OR value->>'command' LIKE 'docker %' OR value->>'command' LIKE 'pip %'
         OR value->>'command' LIKE 'cargo %' OR value->>'command' LIKE 'psql %' OR value->>'command' LIKE 'curl %'
         OR value->>'command' LIKE 'ssh %' OR value->>'command' LIKE 'scp %' OR value->>'command' LIKE 'node %'
         OR value->>'command' LIKE 'npx %' OR value->>'command' LIKE 'claude-flow%' OR value->>'command' LIKE 'ruflo%'
         OR value->>'command' LIKE 'python%' OR value->>'command' LIKE 'find %' OR value->>'command' LIKE 'grep %'
         OR value->>'command' LIKE 'cat %' OR value->>'command' LIKE 'sudo %' OR value->>'command' LIKE 'apt %'
         OR value->>'command' LIKE 'wget %' OR value->>'command' LIKE 'rustup%' OR value->>'command' LIKE 'make%')
  `);
  const hooksTotal = parseInt(hooksCountResult.rows[0].cnt);
  console.log(`  ${hooksTotal} actionable CLI hook entries to embed`);

  while (true) {
    const count = await processBatch(`
      SELECT id, key, namespace, value::text FROM memory_entries
      WHERE source_type = 'hooks' AND embedding IS NULL
      AND key LIKE 'bash:%:pre'
      AND (value->>'command' LIKE 'mkdir%' OR value->>'command' LIKE 'cd %' OR value->>'command' LIKE 'git %'
           OR value->>'command' LIKE 'npm %' OR value->>'command' LIKE 'docker %' OR value->>'command' LIKE 'pip %'
           OR value->>'command' LIKE 'cargo %' OR value->>'command' LIKE 'psql %' OR value->>'command' LIKE 'curl %'
           OR value->>'command' LIKE 'ssh %' OR value->>'command' LIKE 'scp %' OR value->>'command' LIKE 'node %'
           OR value->>'command' LIKE 'npx %' OR value->>'command' LIKE 'claude-flow%' OR value->>'command' LIKE 'ruflo%'
           OR value->>'command' LIKE 'python%' OR value->>'command' LIKE 'find %' OR value->>'command' LIKE 'grep %'
           OR value->>'command' LIKE 'cat %' OR value->>'command' LIKE 'sudo %' OR value->>'command' LIKE 'apt %'
           OR value->>'command' LIKE 'wget %' OR value->>'command' LIKE 'rustup%' OR value->>'command' LIKE 'make%')
      LIMIT 50
    `, 'hooks');
    if (count === 0) break;
    totalEmbedded += count;
    const rate = totalEmbedded / ((Date.now() - startTime) / 1000);
    const remaining = ((hooksTotal - totalEmbedded) / rate / 3600).toFixed(1);
    process.stdout.write(`\r  Hooks: ${totalEmbedded}/${hooksTotal} (${rate.toFixed(1)}/s, ~${remaining}h remaining)  `);
  }
  console.log(`\n  Hooks done: ${totalEmbedded} embedded`);

  // Phase 2: swarm-recovered agent-assignments + file-history
  console.log('=== Phase 2: Swarm-recovered (agent-assignments + file-history) ===');
  const recoveredStart = Date.now();
  let recoveredEmbedded = 0;

  const recoveredCount = await pool.query(`
    SELECT count(id) as cnt FROM memory_entries
    WHERE source_type = 'swarm-recovered' AND embedding IS NULL
    AND (key LIKE 'agent-%' OR key LIKE 'file-history%')
  `);
  const recoveredTotal = parseInt(recoveredCount.rows[0].cnt);
  console.log(`  ${recoveredTotal} swarm-recovered entries to embed`);

  while (true) {
    const count = await processBatch(`
      SELECT id, key, namespace, value::text FROM memory_entries
      WHERE source_type = 'swarm-recovered' AND embedding IS NULL
      AND (key LIKE 'agent-%' OR key LIKE 'file-history%')
      LIMIT 50
    `, 'swarm-recovered');
    if (count === 0) break;
    recoveredEmbedded += count;
    process.stdout.write(`\r  Recovered: ${recoveredEmbedded}/${recoveredTotal}  `);
  }
  console.log(`\n  Recovered done: ${recoveredEmbedded} embedded`);

  totalEmbedded += recoveredEmbedded;
  console.log(`\n=== Complete: ${totalEmbedded} total embedded in ${((Date.now() - startTime) / 1000).toFixed(0)}s ===`);

  const { rows: [{ count }] } = await pool.query('SELECT count(id) as count FROM memory_entries WHERE embedding IS NOT NULL');
  console.log(`Total entries with embeddings: ${count}`);
  await pool.end();
}

main().catch(e => { console.error(e); process.exit(1); });
