/**
 * axiomHandler — axiom add/remove/update via SPARQL or N3 PATCH.
 *
 * All mutations go through patchOntology (SPARQL) or patchOntologyN3 (N3).
 * Cache is invalidated on every successful write.
 */

import { debugState } from '../../../../utils/clientDebugState';
import { createErrorMetadata } from '../../../../utils/loggerConfig';
import { fetchWithAuth, getOntologyUrl, logger } from './contextLoader';
import { SchemaCache, invalidateCache } from './schemaParser';

export interface PatchResult {
  success: boolean;
  status: number;
  statusText: string;
}

/**
 * Represents an RDF term that can be serialized into SPARQL or N3.
 */
export interface RdfTerm {
  value: string;
  /** 'iri' wraps in <>, 'literal' wraps in "", 'prefixed' emits as-is */
  type: 'iri' | 'literal' | 'prefixed';
}

function serializeTerm(term: RdfTerm): string {
  switch (term.type) {
    case 'iri':
      return `<${term.value}>`;
    case 'literal':
      return `"${term.value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
    case 'prefixed':
      return term.value;
  }
}

export async function patchOntology(
  cache: SchemaCache,
  sparqlUpdate: string
): Promise<PatchResult> {
  const url = getOntologyUrl();
  try {
    const response = await fetchWithAuth(url, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/sparql-update' },
      body: sparqlUpdate,
    });

    if (response.ok) invalidateCache(cache);

    if (debugState.isEnabled()) {
      logger.info('SPARQL PATCH sent', { status: response.status, bodyLength: sparqlUpdate.length });
    }

    return { success: response.ok, status: response.status, statusText: response.statusText };
  } catch (error) {
    logger.error('SPARQL PATCH failed', createErrorMetadata(error));
    throw error;
  }
}

export async function patchOntologyN3(
  cache: SchemaCache,
  n3Patch: string
): Promise<PatchResult> {
  const url = getOntologyUrl();
  try {
    const response = await fetchWithAuth(url, {
      method: 'PATCH',
      headers: { 'Content-Type': 'text/n3' },
      body: n3Patch,
    });

    if (response.ok) invalidateCache(cache);

    if (debugState.isEnabled()) {
      logger.info('N3 PATCH sent', { status: response.status, bodyLength: n3Patch.length });
    }

    return { success: response.ok, status: response.status, statusText: response.statusText };
  } catch (error) {
    logger.error('N3 PATCH failed', createErrorMetadata(error));
    throw error;
  }
}

export async function addOntologyTriple(
  cache: SchemaCache,
  subject: RdfTerm,
  predicate: RdfTerm,
  object: RdfTerm
): Promise<PatchResult> {
  const sparql = `INSERT DATA {\n  ${serializeTerm(subject)} ${serializeTerm(predicate)} ${serializeTerm(object)} .\n}`;
  return patchOntology(cache, sparql);
}

export async function removeOntologyTriple(
  cache: SchemaCache,
  subject: RdfTerm,
  predicate: RdfTerm,
  object: RdfTerm
): Promise<PatchResult> {
  const sparql = `DELETE DATA {\n  ${serializeTerm(subject)} ${serializeTerm(predicate)} ${serializeTerm(object)} .\n}`;
  return patchOntology(cache, sparql);
}

export async function updateOntologyTriple(
  cache: SchemaCache,
  subject: RdfTerm,
  predicate: RdfTerm,
  oldValue: RdfTerm,
  newValue: RdfTerm
): Promise<PatchResult> {
  const s = serializeTerm(subject);
  const p = serializeTerm(predicate);
  const sparql = [
    `DELETE { ${s} ${p} ${serializeTerm(oldValue)} . }`,
    `INSERT { ${s} ${p} ${serializeTerm(newValue)} . }`,
    `WHERE  { ${s} ${p} ${serializeTerm(oldValue)} . }`,
  ].join('\n');
  return patchOntology(cache, sparql);
}
