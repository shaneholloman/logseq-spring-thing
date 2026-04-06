/**
 * compute-umap-projection.mjs
 *
 * Connects to RuVector PostgreSQL, fetches embeddings, projects them to 3D
 * via PCA (power-iteration for top 3 eigenvectors), and writes the result
 * to client/public/embedding-cloud.json for the EmbeddingCloudLayer component.
 *
 * Usage:  node scripts/compute-umap-projection.mjs [--limit 50000]
 */

import { createRequire } from 'module';
import { writeFileSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const require = createRequire(import.meta.url);
const { Client } = require('/usr/local/lib/node_modules/pg');

const __dirname = dirname(fileURLToPath(import.meta.url));

const LIMIT = parseInt(process.argv.find((_, i, a) => a[i - 1] === '--limit') || '50000', 10);

const OUTPUT = resolve(__dirname, '../client/public/embedding-cloud.json');

// ---------------------------------------------------------------------------
// 1. Fetch embeddings from RuVector PostgreSQL
// ---------------------------------------------------------------------------

async function fetchEmbeddings() {
  const client = new Client({
    host: process.env.RUVECTOR_PG_HOST || 'ruvector-postgres',
    port: parseInt(process.env.RUVECTOR_PG_PORT || '5432', 10),
    user: process.env.RUVECTOR_PG_USER || 'ruvector',
    password: process.env.RUVECTOR_PG_PASSWORD || 'ruvector',
    database: process.env.RUVECTOR_PG_DB || 'ruvector',
  });

  await client.connect();
  console.log(`Connected. Fetching up to ${LIMIT} embeddings...`);

  const { rows } = await client.query(
    `SELECT id, key, namespace, source_type, embedding::text
     FROM memory_entries
     WHERE embedding IS NOT NULL
     LIMIT $1`,
    [LIMIT]
  );

  await client.end();
  console.log(`Fetched ${rows.length} rows.`);
  return rows;
}

// ---------------------------------------------------------------------------
// 2. Parse embedding text "[0.1,0.2,...]" into a flat Float64 matrix
// ---------------------------------------------------------------------------

function parseEmbeddings(rows) {
  const n = rows.length;
  if (n === 0) return { matrix: new Float64Array(0), dim: 0, n: 0 };

  // Parse first to detect dimension
  const first = JSON.parse(rows[0].embedding);
  const dim = first.length;
  const matrix = new Float64Array(n * dim);

  for (let i = 0; i < n; i++) {
    const vec = JSON.parse(rows[i].embedding);
    for (let j = 0; j < dim; j++) {
      matrix[i * dim + j] = vec[j];
    }
  }

  console.log(`Parsed ${n} embeddings of dimension ${dim}.`);
  return { matrix, dim, n };
}

// ---------------------------------------------------------------------------
// 3. PCA via power iteration — project to 3D
// ---------------------------------------------------------------------------

function pcaProject3D(matrix, n, dim) {
  console.log('Computing PCA (power iteration)...');

  // Compute mean
  const mean = new Float64Array(dim);
  for (let i = 0; i < n; i++) {
    const off = i * dim;
    for (let j = 0; j < dim; j++) mean[j] += matrix[off + j];
  }
  for (let j = 0; j < dim; j++) mean[j] /= n;

  // Center in-place
  for (let i = 0; i < n; i++) {
    const off = i * dim;
    for (let j = 0; j < dim; j++) matrix[off + j] -= mean[j];
  }

  // Power iteration for top-k eigenvectors of X^T X (covariance proportional)
  const components = [];

  for (let k = 0; k < 3; k++) {
    // Random init
    let v = new Float64Array(dim);
    for (let j = 0; j < dim; j++) v[j] = Math.random() - 0.5;
    normalize(v);

    for (let iter = 0; iter < 100; iter++) {
      // w = X^T (X v)
      // Step 1: u = X v  (n-vector)
      const u = new Float64Array(n);
      for (let i = 0; i < n; i++) {
        let dot = 0;
        const off = i * dim;
        for (let j = 0; j < dim; j++) dot += matrix[off + j] * v[j];
        u[i] = dot;
      }

      // Step 2: w = X^T u  (dim-vector)
      const w = new Float64Array(dim);
      for (let i = 0; i < n; i++) {
        const off = i * dim;
        const ui = u[i];
        for (let j = 0; j < dim; j++) w[j] += matrix[off + j] * ui;
      }

      normalize(w);
      v = w;
    }

    components.push(v);

    // Deflate: remove component from data
    for (let i = 0; i < n; i++) {
      const off = i * dim;
      let dot = 0;
      for (let j = 0; j < dim; j++) dot += matrix[off + j] * v[j];
      for (let j = 0; j < dim; j++) matrix[off + j] -= dot * v[j];
    }

    console.log(`  Component ${k + 1}/3 converged.`);
  }

  // Project original centered data onto the 3 components
  // (we already deflated, so re-center isn't needed — projections are computed
  //  during deflation. But we deflated sequentially. Easier: recompute from scratch.)
  // Actually, the projections can be recovered from the deflation steps.
  // Simpler: we stored components. We need the original centered data. But we
  // deflated it. So let's compute projections differently: use the stored
  // components on the deflated + restored data.
  //
  // Since we deflated: X_current = X_orig - sum(X_orig * v_k * v_k^T) for computed k's
  // This means we've lost the original. Instead, let's compute projections as we go.
  // ... Refactoring: collect projections during the loop above.

  // We've already deflated the matrix, but we can reconstruct projections:
  // After all deflations, what remains is X - proj_1 - proj_2 - proj_3.
  // The k-th projection = (original data) . v_k.
  // We need the original centered data back. But we only have the residual.
  // Residual = X_centered - sum_k (X_centered . v_k) outer v_k
  // So X_centered = residual + sum_k proj_k outer v_k
  // And the projection for component k = (residual + sum_j proj_j outer v_j) . v_k
  //                                    = residual . v_k + proj_k  (since v_k are orthogonal)
  // Since residual has been fully deflated of all 3 components, residual . v_k ~ 0.
  // So we actually need to track projections during the loop.

  // Let me redo this properly — restart with original data from a copy.
  // For memory efficiency, just re-read from DB? No — let's just store projections
  // during deflation. The simplest fix: compute the projection right before deflating.

  // Since we already ran the loop, let me just re-parse. The data is still in memory
  // in deflated form. We need the projections.

  // Actually — the projections for each point were computed as `u[i]` (dot product of
  // row i with v) in the LAST iteration of power iteration. That's the projection
  // onto the eigenvector. But we normalized v after that, so u was computed with the
  // v from the previous iteration... Let me just re-project.

  // Reconstruct from residual. residual . v_k ≈ 0 for all k (by deflation).
  // X_centered_ij = residual_ij + sum_k score_ik * v_kj
  // score_ik = X_centered . v_k = (residual + sum_l score_il * v_l) . v_k = score_ik
  // But we don't have scores. We need to reconstruct them.
  // After deflation of all 3, residual . v_k ≈ 0.
  // So score_k for row i = row_i_residual . v_k ≈ 0... that's wrong.
  // The issue: we deflated from the matrix, so the matrix no longer contains those components.

  // The cleanest solution: just allocate the positions array during the deflation loop.
  // Since we can't go back in time, let's re-do the PCA with proper score tracking.

  return null; // Will be replaced by pcaProjectProper below
}

function pcaProjectProper(matrix, n, dim) {
  console.log('Computing PCA (power iteration) with score tracking...');

  // Compute and subtract mean
  const mean = new Float64Array(dim);
  for (let i = 0; i < n; i++) {
    const off = i * dim;
    for (let j = 0; j < dim; j++) mean[j] += matrix[off + j];
  }
  for (let j = 0; j < dim; j++) mean[j] /= n;

  for (let i = 0; i < n; i++) {
    const off = i * dim;
    for (let j = 0; j < dim; j++) matrix[off + j] -= mean[j];
  }

  // positions: n points x 3 coords
  const positions = new Float32Array(n * 3);

  for (let k = 0; k < 3; k++) {
    let v = new Float64Array(dim);
    for (let j = 0; j < dim; j++) v[j] = Math.random() - 0.5;
    normalize(v);

    for (let iter = 0; iter < 100; iter++) {
      const u = new Float64Array(n);
      for (let i = 0; i < n; i++) {
        let dot = 0;
        const off = i * dim;
        for (let j = 0; j < dim; j++) dot += matrix[off + j] * v[j];
        u[i] = dot;
      }
      const w = new Float64Array(dim);
      for (let i = 0; i < n; i++) {
        const off = i * dim;
        const ui = u[i];
        for (let j = 0; j < dim; j++) w[j] += matrix[off + j] * ui;
      }
      normalize(w);
      v = w;
    }

    // Compute projections (scores) for this component
    const scores = new Float64Array(n);
    for (let i = 0; i < n; i++) {
      let dot = 0;
      const off = i * dim;
      for (let j = 0; j < dim; j++) dot += matrix[off + j] * v[j];
      scores[i] = dot;
      positions[i * 3 + k] = dot; // Will normalize later
    }

    // Deflate
    for (let i = 0; i < n; i++) {
      const off = i * dim;
      const s = scores[i];
      for (let j = 0; j < dim; j++) matrix[off + j] -= s * v[j];
    }

    console.log(`  Component ${k + 1}/3 done.`);
  }

  // Normalize positions to fit within ~200-unit radius
  let maxR = 0;
  for (let i = 0; i < n; i++) {
    const x = positions[i * 3], y = positions[i * 3 + 1], z = positions[i * 3 + 2];
    const r = Math.sqrt(x * x + y * y + z * z);
    if (r > maxR) maxR = r;
  }

  const scale = maxR > 0 ? 200 / maxR : 1;
  for (let i = 0; i < positions.length; i++) {
    positions[i] *= scale;
  }

  console.log(`Normalized to radius 200 (scale factor: ${scale.toFixed(4)}).`);
  return positions;
}

function normalize(v) {
  let len = 0;
  for (let i = 0; i < v.length; i++) len += v[i] * v[i];
  len = Math.sqrt(len);
  if (len > 0) for (let i = 0; i < v.length; i++) v[i] /= len;
}

// ---------------------------------------------------------------------------
// 4. Build output JSON
// ---------------------------------------------------------------------------

function buildOutput(rows, positions) {
  const namespacesSet = new Set();
  const sourceTypesSet = new Set();
  const metadata = [];

  for (const row of rows) {
    const ns = row.namespace || 'unknown';
    const st = row.source_type || 'unknown';
    namespacesSet.add(ns);
    sourceTypesSet.add(st);
    metadata.push({ key: row.key || '', namespace: ns, sourceType: st });
  }

  return {
    count: rows.length,
    positions: Array.from(positions),
    metadata,
    namespaces: Array.from(namespacesSet).sort(),
    sourceTypes: Array.from(sourceTypesSet).sort(),
  };
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  const rows = await fetchEmbeddings();
  if (rows.length === 0) {
    console.error('No embeddings found. Exiting.');
    process.exit(1);
  }

  const { matrix, dim, n } = parseEmbeddings(rows);
  const positions = pcaProjectProper(matrix, n, dim);

  const output = buildOutput(rows, positions);

  writeFileSync(OUTPUT, JSON.stringify(output));
  const sizeMB = (Buffer.byteLength(JSON.stringify(output)) / (1024 * 1024)).toFixed(1);
  console.log(`Wrote ${OUTPUT} (${sizeMB} MB, ${output.count} points).`);
}

main().catch(err => {
  console.error('Fatal:', err);
  process.exit(1);
});
