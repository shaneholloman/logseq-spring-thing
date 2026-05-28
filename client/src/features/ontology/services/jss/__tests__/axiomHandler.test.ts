/**
 * axiomHandler tests — only covers the pure helpers (RdfTerm type, serializeTerm indirectly
 * through the SPARQL string builders addOntologyTriple / removeOntologyTriple).
 *
 * Network calls (patchOntology, patchOntologyN3) require a live server and are excluded.
 */
import { describe, it, expect, vi, beforeAll } from 'vitest';
import type { RdfTerm } from '../axiomHandler';

// serializeTerm is unexported; test it indirectly via the SPARQL builders
// We mock the network layer so addOntologyTriple / removeOntologyTriple resolve
// without hitting a real server.

beforeAll(() => {
  // Stub global fetch so patchOntology doesn't fail
  vi.stubGlobal('fetch', async (_url: string, init: RequestInit) => {
    // Echo the body back so tests can inspect the SPARQL string
    const body = init?.body as string;
    return {
      ok: true,
      status: 200,
      statusText: 'OK',
      text: async () => body,
      json: async () => ({}),
    } as any;
  });
});

// Helper to extract the body from an outgoing PATCH
async function getSparqlFrom(
  fn: (cache: any, ...args: any[]) => Promise<any>,
  ...args: any[]
): Promise<string> {
  const cache = { jsonLd: null, timestamp: 0, ttlMs: 60000 };
  const calls: string[] = [];
  const originalFetch = globalThis.fetch;
  vi.stubGlobal('fetch', async (_url: string, init: RequestInit) => {
    calls.push(init.body as string);
    return { ok: true, status: 200, statusText: 'OK' } as any;
  });
  await fn(cache, ...args);
  vi.stubGlobal('fetch', originalFetch);
  return calls[0] ?? '';
}

describe('serializeTerm (via addOntologyTriple)', () => {
  it('IRI terms are wrapped in angle brackets', async () => {
    const { addOntologyTriple } = await import('../axiomHandler');
    const sparql = await getSparqlFrom(
      addOntologyTriple,
      { type: 'iri', value: 'http://example.org/A' } as RdfTerm,
      { type: 'iri', value: 'http://example.org/B' } as RdfTerm,
      { type: 'iri', value: 'http://example.org/C' } as RdfTerm,
    );
    expect(sparql).toContain('<http://example.org/A>');
    expect(sparql).toContain('<http://example.org/B>');
    expect(sparql).toContain('<http://example.org/C>');
  });

  it('literal terms are wrapped in double quotes', async () => {
    const { addOntologyTriple } = await import('../axiomHandler');
    const sparql = await getSparqlFrom(
      addOntologyTriple,
      { type: 'iri', value: 'ex:subj' } as RdfTerm,
      { type: 'iri', value: 'ex:pred' } as RdfTerm,
      { type: 'literal', value: 'hello world' } as RdfTerm,
    );
    expect(sparql).toContain('"hello world"');
  });

  it('prefixed terms are emitted as-is', async () => {
    const { addOntologyTriple } = await import('../axiomHandler');
    const sparql = await getSparqlFrom(
      addOntologyTriple,
      { type: 'prefixed', value: 'owl:Class' } as RdfTerm,
      { type: 'prefixed', value: 'rdf:type' } as RdfTerm,
      { type: 'prefixed', value: 'owl:Thing' } as RdfTerm,
    );
    expect(sparql).toContain('owl:Class');
    expect(sparql).toContain('rdf:type');
    expect(sparql).toContain('owl:Thing');
  });

  it('literals escape backslashes and double quotes', async () => {
    const { addOntologyTriple } = await import('../axiomHandler');
    const sparql = await getSparqlFrom(
      addOntologyTriple,
      { type: 'iri', value: 'ex:s' } as RdfTerm,
      { type: 'iri', value: 'ex:p' } as RdfTerm,
      { type: 'literal', value: 'say "hello"' } as RdfTerm,
    );
    expect(sparql).toContain('\\"hello\\"');
  });
});

describe('removeOntologyTriple SPARQL shape', () => {
  it('emits DELETE DATA { ... }', async () => {
    const { removeOntologyTriple } = await import('../axiomHandler');
    const sparql = await getSparqlFrom(
      removeOntologyTriple,
      { type: 'iri', value: 'ex:s' } as RdfTerm,
      { type: 'iri', value: 'ex:p' } as RdfTerm,
      { type: 'iri', value: 'ex:o' } as RdfTerm,
    );
    expect(sparql).toMatch(/DELETE DATA/);
  });
});

describe('addOntologyTriple SPARQL shape', () => {
  it('emits INSERT DATA { ... }', async () => {
    const { addOntologyTriple } = await import('../axiomHandler');
    const sparql = await getSparqlFrom(
      addOntologyTriple,
      { type: 'iri', value: 'ex:s' } as RdfTerm,
      { type: 'iri', value: 'ex:p' } as RdfTerm,
      { type: 'iri', value: 'ex:o' } as RdfTerm,
    );
    expect(sparql).toMatch(/INSERT DATA/);
  });
});
