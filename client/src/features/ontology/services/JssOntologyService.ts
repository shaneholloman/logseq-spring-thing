/**
 * JSS Ontology Service — facade
 *
 * Provides live ontology data integration with JavaScript Solid Server (JSS).
 * Implementation is split across:
 *   jss/contextLoader.ts   — URL config, authenticated fetch
 *   jss/schemaParser.ts    — JSON-LD + Turtle fetch with TTL cache
 *   jss/classExtractor.ts  — OWL hierarchy + metrics derivation
 *   jss/axiomHandler.ts    — SPARQL/N3 PATCH mutations
 *   jss/inferenceClient.ts — WebSocket real-time subscriptions
 *
 * Public API is unchanged.
 */

import { debugState } from '../../../utils/clientDebugState';
import { useOntologyStore, OntologyHierarchy } from '../store/useOntologyStore';
import { logger } from './jss/contextLoader';
import {
  SchemaCache,
  FetchOptions,
  JsonLdContext,
  JsonLdOntology,
  JsonLdNode,
  makeSchemaCache,
  isCacheValid,
  invalidateCache,
  fetchJsonLd,
  fetchTurtle,
} from './jss/schemaParser';
import { buildHierarchyFromJsonLd, extractMetricsFromJsonLd } from './jss/classExtractor';
import {
  PatchResult,
  RdfTerm,
  patchOntology,
  patchOntologyN3,
  addOntologyTriple,
  removeOntologyTriple,
  updateOntologyTriple,
} from './jss/axiomHandler';
import {
  OntologyChangeEvent,
  OntologyChangeCallback,
  InferenceClient,
} from './jss/inferenceClient';

// Re-export types so callers that import from this module still work.
export type {
  JsonLdContext,
  JsonLdOntology,
  JsonLdNode,
  FetchOptions,
  PatchResult,
  RdfTerm,
  OntologyChangeEvent,
  OntologyChangeCallback,
};

class JssOntologyService {
  private static instance: JssOntologyService;

  private readonly cache: SchemaCache = makeSchemaCache();
  private readonly metrics = { fetchCount: 0, cacheHitCount: 0, lastFetchDurationMs: 0 };
  private readonly ws: InferenceClient = new InferenceClient();

  private constructor() {}

  public static getInstance(): JssOntologyService {
    if (!JssOntologyService.instance) {
      JssOntologyService.instance = new JssOntologyService();
    }
    return JssOntologyService.instance;
  }

  // --- JSON-LD Fetching ---

  public async fetchOntologyJsonLd(options: FetchOptions = {}): Promise<JsonLdOntology> {
    return fetchJsonLd(this.cache, this.metrics, options);
  }

  public async fetchOntologyHierarchy(options: FetchOptions = {}): Promise<OntologyHierarchy> {
    const jsonLd = await this.fetchOntologyJsonLd(options);
    return buildHierarchyFromJsonLd(jsonLd);
  }

  // --- Turtle Fetching ---

  public async fetchOntologyTurtle(options: FetchOptions = {}): Promise<string> {
    return fetchTurtle(this.cache, this.metrics, options);
  }

  // --- WebSocket Real-time Updates ---

  public connectWebSocket(): void {
    this.ws.connect();
    this.ws.bindRefresh(this.cache, this.metrics);
  }

  public onResourceChange(callback: OntologyChangeCallback): () => void {
    return this.ws.onResourceChange(callback);
  }

  public disconnect(): void {
    this.ws.disconnect();
  }

  // --- Store Integration ---

  public async loadIntoStore(options: FetchOptions = {}): Promise<void> {
    const store = useOntologyStore.getState();
    store.setValidating(true);

    try {
      const jsonLd = await this.fetchOntologyJsonLd(options);
      const hierarchy = buildHierarchyFromJsonLd(jsonLd);
      const metrics = extractMetricsFromJsonLd(
        jsonLd,
        this.metrics.fetchCount,
        this.metrics.cacheHitCount,
        this.metrics.lastFetchDurationMs
      );

      store.setHierarchy(hierarchy);
      store.setMetrics(metrics);
      store.setLoaded(true);

      if (debugState.isEnabled()) {
        logger.info('Ontology loaded into store', {
          classCount: hierarchy.classes.size,
          rootCount: hierarchy.roots.length,
        });
      }
    } catch (error) {
      throw error;
    } finally {
      store.setValidating(false);
    }
  }

  public async initialize(): Promise<void> {
    await this.loadIntoStore();
    this.connectWebSocket();
  }

  // --- SPARQL / N3 PATCH Mutations ---

  public async patchOntology(sparqlUpdate: string): Promise<PatchResult> {
    return patchOntology(this.cache, sparqlUpdate);
  }

  public async patchOntologyN3(n3Patch: string): Promise<PatchResult> {
    return patchOntologyN3(this.cache, n3Patch);
  }

  public async addOntologyTriple(
    subject: RdfTerm,
    predicate: RdfTerm,
    object: RdfTerm
  ): Promise<PatchResult> {
    return addOntologyTriple(this.cache, subject, predicate, object);
  }

  public async removeOntologyTriple(
    subject: RdfTerm,
    predicate: RdfTerm,
    object: RdfTerm
  ): Promise<PatchResult> {
    return removeOntologyTriple(this.cache, subject, predicate, object);
  }

  public async updateOntologyTriple(
    subject: RdfTerm,
    predicate: RdfTerm,
    oldValue: RdfTerm,
    newValue: RdfTerm
  ): Promise<PatchResult> {
    return updateOntologyTriple(this.cache, subject, predicate, oldValue, newValue);
  }

  // --- Public Getters ---

  public isConnected(): boolean {
    return this.ws.connected;
  }

  public getCacheStats(): { hits: number; age: number; valid: boolean } {
    return {
      hits: this.metrics.fetchCount,
      age: this.cache.timestamp > 0 ? Date.now() - this.cache.timestamp : -1,
      valid: isCacheValid(this.cache),
    };
  }

  public getLastFetchDuration(): number {
    return this.metrics.lastFetchDurationMs;
  }
}

export const jssOntologyService = JssOntologyService.getInstance();
export default jssOntologyService;
