/**
 * Ontology-physics service — the client's window onto the GPU-resident
 * constraint engine (PRD-018 / ADR-098). All semantic solving runs on the
 * CUDA live-kernel server-side; this service only reads live constraint stats
 * and adjusts strength/enable knobs. It performs NO layout and NO solving.
 *
 * Endpoints (all server-side, GPU-resident):
 *   GET  /api/ontology-physics/constraints  → live constraint + axiom stats
 *   PUT  /api/ontology-physics/weights       → global force strength
 *   POST /api/ontology-physics/enable        → re-dispatch constraints
 *   POST /api/ontology-physics/disable       → clear constraints
 *   POST /api/admin/sync                      → re-ingest + re-reason + re-dispatch
 */

import { unifiedApiClient } from '@/services/api/UnifiedApiClient';
import { createLogger } from '@/utils/loggerConfig';

const logger = createLogger('OntologyPhysicsService');

// Paths are relative to the unifiedApiClient base ('/api'); a leading '/api'
// here would double to '/api/api/...' and 404.
const CONSTRAINTS_ENDPOINT = '/ontology-physics/constraints';
const WEIGHTS_ENDPOINT = '/ontology-physics/weights';
const ENABLE_ENDPOINT = '/ontology-physics/enable';
const DISABLE_ENDPOINT = '/ontology-physics/disable';
const RESYNC_ENDPOINT = '/admin/sync';

/** Live GPU constraint statistics (mirrors OntologyConstraintStats server-side). */
export interface ConstraintStats {
  /** Constraints currently driving GPU forces. */
  activeConstraints: number;
  /** Total constraints uploaded (== active post-dispatch). */
  totalConstraints: number;
  /** Distinct OWL axioms consumed by the mapper. */
  axiomsProcessed: number;
  /** GPU constraint-evaluation passes since last dispatch. */
  constraintEvaluationCount: number;
  /** Wall-clock ms of the last constraint upload. */
  lastUpdateTimeMs: number;
  /** Count of GPU upload/eval failures (health telltale). */
  gpuFailureCount: number;
  /** Count of CPU-fallback evaluations (health telltale). */
  cpuFallbackCount: number;
}

export const EMPTY_CONSTRAINT_STATS: ConstraintStats = {
  activeConstraints: 0,
  totalConstraints: 0,
  axiomsProcessed: 0,
  constraintEvaluationCount: 0,
  lastUpdateTimeMs: 0,
  gpuFailureCount: 0,
  cpuFallbackCount: 0,
};

interface RawConstraintStats {
  activeConstraints?: number;
  totalConstraints?: number;
  axiomsProcessed?: number;
  constraintEvaluationCount?: number;
  lastUpdateTimeMs?: number;
  gpuFailureCount?: number;
  cpuFallbackCount?: number;
}

const num = (v: unknown): number => (typeof v === 'number' && Number.isFinite(v) ? v : 0);

/**
 * Fetch live GPU constraint stats. Empty-safe: returns EMPTY_CONSTRAINT_STATS
 * (never throws) so pollers degrade to a "0 constraints" reading rather than
 * surfacing errors when the GPU manager is briefly unavailable.
 */
export async function fetchConstraintStats(): Promise<ConstraintStats> {
  try {
    const res = await unifiedApiClient.get<RawConstraintStats>(CONSTRAINTS_ENDPOINT, { timeout: 8000 });
    const body = res.data as RawConstraintStats & { data?: RawConstraintStats };
    const d: RawConstraintStats = body?.data ?? body ?? {};
    return {
      activeConstraints: num(d.activeConstraints),
      totalConstraints: num(d.totalConstraints),
      axiomsProcessed: num(d.axiomsProcessed),
      constraintEvaluationCount: num(d.constraintEvaluationCount),
      lastUpdateTimeMs: num(d.lastUpdateTimeMs),
      gpuFailureCount: num(d.gpuFailureCount),
      cpuFallbackCount: num(d.cpuFallbackCount),
    };
  } catch (err: any) {
    logger.debug('Constraint stats unavailable; returning empty:', err?.message);
    return { ...EMPTY_CONSTRAINT_STATS };
  }
}

/** Set the global ontology force strength (0..1). Returns success. */
export async function setForceStrength(globalStrength: number): Promise<boolean> {
  const clamped = Math.max(0, Math.min(1, globalStrength));
  try {
    await unifiedApiClient.put(WEIGHTS_ENDPOINT, { globalStrength: clamped });
    logger.info(`Ontology force strength set to ${clamped.toFixed(2)}`);
    return true;
  } catch (err: any) {
    logger.warn('Failed to set force strength:', err?.message);
    return false;
  }
}

/** Enable ontology forces (re-dispatch the live constraint set to the GPU). */
export async function enableForces(strength = 0.8): Promise<boolean> {
  try {
    await unifiedApiClient.post(ENABLE_ENDPOINT, { ontologyId: 'default', strength });
    logger.info('Ontology forces enabled');
    return true;
  } catch (err: any) {
    logger.warn('Failed to enable ontology forces:', err?.message);
    return false;
  }
}

/** Disable ontology forces (clear the GPU constraint set). */
export async function disableForces(): Promise<boolean> {
  try {
    await unifiedApiClient.post(DISABLE_ENDPOINT);
    logger.info('Ontology forces disabled');
    return true;
  } catch (err: any) {
    logger.warn('Failed to disable ontology forces:', err?.message);
    return false;
  }
}

/**
 * Trigger a full re-ingest + re-reason + constraint re-dispatch. This re-runs
 * the GitHub→Oxigraph→Whelk→GPU pipeline server-side; the new constraints flow
 * to the live kernel automatically. Power-user / dev-token gated server-side.
 * Resolves to true on 2xx.
 */
export async function resyncReasoning(): Promise<boolean> {
  try {
    await unifiedApiClient.post(RESYNC_ENDPOINT);
    logger.info('Re-sync reasoning triggered');
    return true;
  } catch (err: any) {
    logger.warn('Failed to trigger re-sync:', err?.message);
    return false;
  }
}
