/**
 * inferenceClient — WebSocket subscription to JSS Solid notifications.
 *
 * Connects via webSocketService (solid-0.1 protocol), invalidates the
 * schema cache on change notifications, refetches, and fans out to
 * registered OntologyChangeCallbacks.
 */

import { debugState } from '../../../../utils/clientDebugState';
import { createErrorMetadata } from '../../../../utils/loggerConfig';
import { webSocketService, type SolidNotification } from '../../../../store/websocketStore';
import { useOntologyStore } from '../../store/useOntologyStore';
import { JSS_WS_URL, getOntologyUrl, logger } from './contextLoader';
import { SchemaCache, FetchOptions, invalidateCache, fetchJsonLd } from './schemaParser';
import { buildHierarchyFromJsonLd } from './classExtractor';

export interface OntologyChangeEvent {
  type: 'class_added' | 'class_removed' | 'property_added' | 'property_removed' | 'full_refresh';
  resourceUrl: string;
  timestamp: number;
  data?: import('./schemaParser').JsonLdNode;
}

export type OntologyChangeCallback = (event: OntologyChangeEvent) => void;

export class InferenceClient {
  private changeCallbacks: Set<OntologyChangeCallback> = new Set();
  private unsubscribeFn: (() => void) | null = null;
  private isSubscribed: boolean = false;

  connect(): void {
    if (!JSS_WS_URL) {
      logger.warn('JSS WebSocket URL not configured (VITE_JSS_WS_URL)');
      return;
    }
    webSocketService.connectSolid();
    if (!this.isSubscribed) this.subscribe();
  }

  private subscribe(): void {
    const ontologyUrl = getOntologyUrl();
    if (debugState.isEnabled()) logger.info('Subscribing to ontology changes', { url: ontologyUrl });

    if (this.unsubscribeFn) this.unsubscribeFn();

    this.unsubscribeFn = webSocketService.subscribeSolidResource(
      ontologyUrl,
      (notification: SolidNotification) => {
        void this.handleNotification(notification);
      }
    );
    this.isSubscribed = true;
  }

  private async handleNotification(
    notification: SolidNotification,
    cache?: SchemaCache,
    metrics?: { fetchCount: number; cacheHitCount: number; lastFetchDurationMs: number },
    fetchOptions?: FetchOptions
  ): Promise<void> {
    if (notification.type !== 'pub') return;
    if (debugState.isEnabled()) logger.info('Ontology resource changed', { url: notification.url });

    if (cache) invalidateCache(cache);

    try {
      if (cache && metrics) {
        const jsonLd = await fetchJsonLd(cache, metrics, { ...fetchOptions, skipCache: true });
        const hierarchy = buildHierarchyFromJsonLd(jsonLd);
        const store = useOntologyStore.getState();
        store.setHierarchy(hierarchy);
        store.setLoaded(true);
      }

      const event: OntologyChangeEvent = {
        type: 'full_refresh',
        resourceUrl: notification.url,
        timestamp: Date.now(),
      };
      this.notifyCallbacks(event);

      if (debugState.isEnabled()) logger.info('Ontology store updated from WebSocket notification');
    } catch (error) {
      logger.error('Failed to refresh ontology after change notification', createErrorMetadata(error));
    }
  }

  /**
   * Bind the cache and metrics so handleNotification can perform a full refresh.
   * Called by the facade immediately after construction.
   */
  bindRefresh(
    cache: SchemaCache,
    metrics: { fetchCount: number; cacheHitCount: number; lastFetchDurationMs: number }
  ): void {
    const ontologyUrl = getOntologyUrl();

    if (this.unsubscribeFn) this.unsubscribeFn();

    this.unsubscribeFn = webSocketService.subscribeSolidResource(
      ontologyUrl,
      (notification: SolidNotification) => {
        void this.handleNotification(notification, cache, metrics);
      }
    );
    this.isSubscribed = true;
  }

  onResourceChange(callback: OntologyChangeCallback): () => void {
    this.changeCallbacks.add(callback);
    return () => this.changeCallbacks.delete(callback);
  }

  private notifyCallbacks(event: OntologyChangeEvent): void {
    this.changeCallbacks.forEach((cb) => {
      try {
        cb(event);
      } catch (error) {
        logger.error('Error in ontology change callback', createErrorMetadata(error));
      }
    });
  }

  disconnect(): void {
    if (this.unsubscribeFn) {
      this.unsubscribeFn();
      this.unsubscribeFn = null;
    }
    this.isSubscribed = false;
    this.changeCallbacks.clear();
  }

  get connected(): boolean {
    return this.isSubscribed && webSocketService.isSolidWebSocketConnected();
  }
}
