/**
 * SPARQL Service — read-only SELECT over the server-side Oxigraph store.
 *
 * BINDING CONSTRAINT (PRD-018 §"Binding constraints"): the client NEVER
 * executes SPARQL locally. There is no Comunica/in-browser engine here. This
 * module only POSTs a query string to a server endpoint and renders whatever
 * the server returns. All solving stays server-side over Oxigraph.
 *
 * The query is gated to SELECT-only on the client as a UX guard; the server
 * remains the authoritative read-only boundary.
 *
 * Expected backend contract (handed to the Rust agents — see report):
 *   POST /api/ontology/sparql
 *   Request  (application/json): { "query": "SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10" }
 *   Response (application/json): SPARQL 1.1 JSON Results shape:
 *     {
 *       "head": { "vars": ["s", "p", "o"] },
 *       "results": { "bindings": [ { "s": { "type": "uri", "value": "..." }, ... } ] }
 *     }
 *   Non-200 → { "error": "message" } body; surfaced to the user, never faked.
 */

import { unifiedApiClient } from '@/services/api/UnifiedApiClient';
import { createLogger } from '@/utils/loggerConfig';

const logger = createLogger('sparqlService');

/** Canonical endpoint path the backend SPARQL agent must implement.
 *  Relative to the unifiedApiClient base ('/api'); a leading '/api' here doubles to 404. */
export const SPARQL_ENDPOINT = '/ontology/sparql';

/** A single RDF term in a SPARQL JSON result binding. */
export interface SparqlTerm {
  type: 'uri' | 'literal' | 'bnode' | 'typed-literal';
  value: string;
  'xml:lang'?: string;
  datatype?: string;
}

/** SPARQL 1.1 JSON Results object. */
export interface SparqlSelectResult {
  head: { vars: string[] };
  results: { bindings: Array<Record<string, SparqlTerm>> };
}

export interface SparqlQueryOutcome {
  /** Ordered variable names from the result head (table columns). */
  vars: string[];
  /** One row per binding; cell keyed by var name (absent = unbound). */
  rows: Array<Record<string, SparqlTerm | undefined>>;
  /** Number of rows returned. */
  rowCount: number;
}

/** SELECT-only client guard. Rejects mutating / non-SELECT forms early so the
 *  read-only contract is visible at the call site rather than relying solely on
 *  the server to refuse. */
const FORBIDDEN = /\b(INSERT|DELETE|LOAD|CLEAR|DROP|CREATE|ADD|MOVE|COPY|WITH)\b/i;

export function isReadOnlySelect(query: string): boolean {
  const trimmed = query.trim();
  if (FORBIDDEN.test(trimmed)) return false;
  // Allow a leading set of PREFIX declarations before SELECT/ASK.
  const body = trimmed.replace(/^(\s*PREFIX\s+[^\n]+\n?)*/i, '').trim();
  return /^(SELECT|ASK)\b/i.test(body);
}

/**
 * Execute a read-only SELECT against the server-side Oxigraph endpoint.
 * Throws on a client-side guard failure; returns a normalised outcome on 2xx;
 * raises with the server-provided message on non-200.
 */
export async function runSparqlSelect(query: string): Promise<SparqlQueryOutcome> {
  if (!isReadOnlySelect(query)) {
    throw new Error('Only read-only SELECT/ASK queries are permitted from the console.');
  }

  try {
    const response = await unifiedApiClient.post<SparqlSelectResult | { error: string }>(
      SPARQL_ENDPOINT,
      { query },
      { timeout: 30000 },
    );

    const data = response.data as Partial<SparqlSelectResult> & { error?: string };
    if (data?.error) {
      throw new Error(data.error);
    }

    const vars = Array.isArray(data?.head?.vars) ? data!.head!.vars : [];
    const bindings = Array.isArray(data?.results?.bindings) ? data!.results!.bindings : [];
    const rows = bindings.map((b) => {
      const row: Record<string, SparqlTerm | undefined> = {};
      for (const v of vars) row[v] = b[v];
      return row;
    });

    logger.info(`SPARQL SELECT returned ${rows.length} rows over ${vars.length} columns`);
    return { vars, rows, rowCount: rows.length };
  } catch (err: any) {
    // Surface a server message when present (non-200 path), else the raw error.
    const serverMsg = err?.response?.data?.error || err?.data?.error;
    const message = serverMsg || err?.message || 'SPARQL request failed';
    logger.error('SPARQL request failed:', message);
    throw new Error(message);
  }
}
