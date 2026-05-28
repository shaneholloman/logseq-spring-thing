#!/usr/bin/env node
/**
 * Import SQLite memory.db files into RuVector PostgreSQL
 *
 * Maps SQLite memory_entries schema → PG memory_entries schema
 * Deduplicates via ON CONFLICT on (project_id, namespace, key)
 * Prefixes namespace with source path for provenance tracking
 */
import Database from 'better-sqlite3';
import pg from 'pg';
import { randomUUID } from 'crypto';
import { readdirSync, statSync, existsSync } from 'fs';
import { basename, dirname, resolve } from 'path';

const { Pool } = pg;

const pool = new Pool({
  host: process.env.PGHOST || 'ruvector-postgres',
  port: parseInt(process.env.PGPORT || '5432'),
  user: process.env.PGUSER || 'ruvector',
  password: process.env.PGPASSWORD || 'ruvector',
  database: process.env.PGDATABASE || 'ruvector',
  max: 5,
});

// Map workspace paths to project IDs (from projects table)
const PROJECT_MAP = {
  'blender-mcp': 1,
  'dreamlab-cumbria': 2,
  'dreamlab-ai-website-backup': 3,
  'heatmiser': 5,
  'WasmVOWL': 6,
  'JavaScriptSolidServer': 7,
  'workspace-root': 9,
  'hackathon-tv5': 10,
  'OntologyDesign': 11,
  'report-Pete': 12,
  'project-claude': 13,
  'project/client': 14,
  'project2': 15,
  'logseq': 27,
};

function getProjectId(dbPath) {
  const rel = dbPath.replace('/home/devuser/workspace/', '');
  // Check direct matches
  for (const [key, id] of Object.entries(PROJECT_MAP)) {
    if (rel.startsWith(key + '/') || rel.startsWith(key + '.')) return id;
  }
  // Workspace root
  if (rel.startsWith('.swarm/') || rel.startsWith('.hive-mind/') || rel.startsWith('.agentic-qe/')) return 9;
  // Project root
  if (rel.startsWith('project/')) return 13;
  // Default
  return 9;
}

function getSourcePrefix(dbPath) {
  const rel = dbPath.replace('/home/devuser/workspace/', '');
  const parts = rel.split('/');
  // e.g. "project/.swarm/memory.db" → "legacy/project/swarm"
  const proj = parts[0];
  const store = parts.find(p => p.startsWith('.')) || 'unknown';
  return `legacy/${proj}/${store.replace('.', '')}`;
}

function epochToTimestamp(epoch) {
  if (!epoch) return new Date().toISOString();
  // SQLite stores as seconds or milliseconds
  const ms = epoch > 1e12 ? epoch : epoch * 1000;
  return new Date(ms).toISOString();
}

function safeJsonb(val) {
  if (!val) return null;
  try {
    JSON.parse(val);
    return val;
  } catch {
    return JSON.stringify(val);
  }
}

async function importSqliteDb(dbPath) {
  let db;
  try {
    db = new Database(dbPath, { readonly: true });
  } catch (e) {
    console.error(`  Skip (can't open): ${dbPath}: ${e.message}`);
    return { imported: 0, skipped: 0, errors: 0 };
  }

  let rows;
  try {
    rows = db.prepare('SELECT * FROM memory_entries WHERE status != "deleted" OR status IS NULL').all();
  } catch {
    try {
      rows = db.prepare('SELECT * FROM memory_entries').all();
    } catch (e) {
      console.error(`  Skip (no memory_entries): ${dbPath}`);
      db.close();
      return { imported: 0, skipped: 0, errors: 0 };
    }
  }

  if (rows.length === 0) {
    db.close();
    return { imported: 0, skipped: 0, errors: 0 };
  }

  const projectId = getProjectId(dbPath);
  const sourcePrefix = getSourcePrefix(dbPath);
  const client = await pool.connect();
  let imported = 0, skipped = 0, errors = 0;

  try {
    // Batch insert in chunks of 500
    const BATCH = 500;
    for (let i = 0; i < rows.length; i += BATCH) {
      const batch = rows.slice(i, i + BATCH);
      const values = [];
      const placeholders = [];
      let paramIdx = 1;

      for (const row of batch) {
        const id = randomUUID();
        const key = row.key || `entry-${row.id}`;
        const ns = row.namespace || 'default';
        const value = safeJsonb(row.value || row.content || '""');
        const metadata = safeJsonb(row.metadata) || '{}';
        const createdAt = epochToTimestamp(row.created_at);
        const updatedAt = epochToTimestamp(row.updated_at);
        const accessCount = row.access_count || 0;

        placeholders.push(`($${paramIdx}, $${paramIdx+1}, $${paramIdx+2}, $${paramIdx+3}, $${paramIdx+4}::jsonb, $${paramIdx+5}::jsonb, $${paramIdx+6}, $${paramIdx+7}::timestamp, $${paramIdx+8}::timestamp, $${paramIdx+9})`);
        values.push(id, projectId, `${sourcePrefix}/${ns}`, key, value, metadata, 'sqlite-import', createdAt, updatedAt, accessCount);
        paramIdx += 10;
      }

      try {
        const result = await client.query(`
          INSERT INTO memory_entries (id, project_id, namespace, key, value, metadata, source_type, created_at, updated_at, access_count)
          VALUES ${placeholders.join(',')}
          ON CONFLICT ON CONSTRAINT memory_entries_project_id_namespace_key_key DO NOTHING
        `, values);
        imported += result.rowCount;
        skipped += batch.length - result.rowCount;
      } catch (e) {
        // Fallback: insert one by one
        for (const row of batch) {
          try {
            const id = randomUUID();
            const key = row.key || `entry-${row.id}`;
            const ns = row.namespace || 'default';
            const value = safeJsonb(row.value || row.content || '""');
            const metadata = safeJsonb(row.metadata) || '{}';
            const createdAt = epochToTimestamp(row.created_at);
            const updatedAt = epochToTimestamp(row.updated_at);

            const r = await client.query(`
              INSERT INTO memory_entries (id, project_id, namespace, key, value, metadata, source_type, created_at, updated_at, access_count)
              VALUES ($1, $2, $3, $4, $5::jsonb, $6::jsonb, $7, $8::timestamp, $9::timestamp, $10)
              ON CONFLICT ON CONSTRAINT memory_entries_project_id_namespace_key_key DO NOTHING
            `, [id, projectId, `${sourcePrefix}/${ns}`, key, value, metadata, 'sqlite-import', createdAt, updatedAt, row.access_count || 0]);
            if (r.rowCount > 0) imported++; else skipped++;
          } catch (e2) {
            errors++;
          }
        }
      }
    }
  } finally {
    client.release();
  }

  db.close();
  return { imported, skipped, errors };
}

async function main() {
  // Find all memory.db files
  const dbFiles = [];
  function findDbs(dir) {
    try {
      for (const entry of readdirSync(dir)) {
        if (entry === 'node_modules' || entry === '.git') continue;
        const full = resolve(dir, entry);
        try {
          const stat = statSync(full);
          if (stat.isDirectory()) findDbs(full);
          else if (entry === 'memory.db' && stat.size > 0) dbFiles.push(full);
        } catch {}
      }
    } catch {}
  }

  findDbs('/home/devuser/workspace');

  // Sort by size descending (process biggest first)
  dbFiles.sort((a, b) => {
    try { return statSync(b).size - statSync(a).size; } catch { return 0; }
  });

  // Deduplicate (skip .v2.backup copies)
  const seen = new Set();
  const uniqueDbs = dbFiles.filter(f => {
    if (f.includes('.v2.backup')) return false;
    const key = basename(dirname(dirname(f))) + '/' + basename(dirname(f));
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });

  console.log(`Found ${uniqueDbs.length} SQLite databases to import`);

  let totalImported = 0, totalSkipped = 0, totalErrors = 0;

  for (const dbPath of uniqueDbs) {
    const size = (statSync(dbPath).size / 1048576).toFixed(1);
    process.stdout.write(`  ${dbPath} (${size} MB)... `);
    const { imported, skipped, errors } = await importSqliteDb(dbPath);
    console.log(`+${imported} new, ${skipped} dupes, ${errors} errors`);
    totalImported += imported;
    totalSkipped += skipped;
    totalErrors += errors;
  }

  console.log(`\nDone: ${totalImported} imported, ${totalSkipped} duplicates skipped, ${totalErrors} errors`);

  // Final count
  const client = await pool.connect();
  const result = await client.query('SELECT count(id) as total FROM memory_entries');
  console.log(`Total entries in PG: ${result.rows[0].total}`);
  client.release();

  await pool.end();
}

main().catch(e => { console.error(e); process.exit(1); });
