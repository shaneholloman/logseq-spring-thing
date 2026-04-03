/**
 * Solid Pod Service
 *
 * Provides integration with JavaScript Solid Server (JSS) for:
 * - Pod management (create, check, access)
 * - LDP CRUD operations
 * - WebSocket notifications (solid-0.1 protocol)
 * - Content negotiation (JSON-LD, Turtle)
 *
 * Works with VisionFlow's Nostr authentication system.
 *
 * TECH DEBT: This file is ~1600 lines (3.2x over 500-line limit).
 * Graph view methods have been extracted to SolidGraphViewService.ts and
 * re-exported here for backward compatibility. Further decomposition
 * (WebSocket management, LDP CRUD, type index operations) should follow
 * the same pattern when touching those areas.
 */

import { createLogger } from '../utils/loggerConfig';
import { nostrAuth } from './nostrAuthService';
import { webSocketRegistry } from './WebSocketRegistry';
import { webSocketEventBus } from './WebSocketEventBus';

const logger = createLogger('SolidPodService');

const REGISTRY_NAME = 'solid-pod';

// --- Interfaces ---

/**
 * Pod directory structure matching backend PodStructure
 */
export interface PodStructure {
  profile: string;
  ontology_contributions: string;
  ontology_proposals: string;
  ontology_annotations: string;
  preferences: string;
  inbox: string;
}

export interface PodInfo {
  exists: boolean;
  podUrl?: string;
  webId?: string;
  suggestedUrl?: string;
  structure?: PodStructure;
}

export interface PodCreationResult {
  success: boolean;
  podUrl?: string;
  webId?: string;
  created?: boolean;
  structure?: PodStructure;
  error?: string;
}

export interface PodInitResult {
  success: boolean;
  podUrl?: string;
  webId?: string;
  created: boolean;
  structure?: PodStructure;
  npub?: string;
  error?: string;
}

export interface JsonLdDocument {
  '@context': string | object;
  '@type'?: string;
  '@id'?: string;
  [key: string]: unknown;
}

export interface SolidNotification {
  type: 'pub' | 'ack';
  url: string;
}

export interface TypeRegistration {
  '@type': string;
  'solid:forClass': string;
  'solid:instance'?: string;
  'solid:instanceContainer'?: string;
  [key: string]: unknown;
}

export interface TypeIndexDocument extends JsonLdDocument {
  '@type': 'solid:TypeIndex';
  'solid:typeRegistration': TypeRegistration[];
}

export interface DiscoveredView {
  name: string;
  url: string;
}

export interface DiscoveredAgent {
  id: string;
  capabilities: string[];
}

type NotificationCallback = (notification: SolidNotification) => void;

// --- Configuration ---

const JSS_BASE_URL = import.meta.env.VITE_JSS_URL || '/solid';
const JSS_WS_URL = import.meta.env.VITE_JSS_WS_URL || null;

// --- Service Implementation ---

/**
 * Sanitize a preference key to prevent path traversal.
 * Strips path separators and dot-sequences that could escape the container.
 */
function sanitizePreferenceKey(key: string): string {
  // Remove sequences of two or more dots/slashes/backslashes (path traversal)
  let sanitized = key.replace(/[\/\\\.]{2,}/g, '');
  // Replace any remaining slashes/backslashes with hyphens
  sanitized = sanitized.replace(/[\/\\]/g, '-');
  // Remove leading dots or hyphens
  sanitized = sanitized.replace(/^[.\-]+/, '');
  if (!sanitized) {
    throw new Error('Invalid preference key');
  }
  return sanitized;
}

class SolidPodService {
  private static instance: SolidPodService;
  private wsConnection: WebSocket | null = null;
  private subscriptions: Map<string, Set<NotificationCallback>> = new Map();
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000;
  private reconnectTimerId: ReturnType<typeof setTimeout> | null = null;
  private isDisconnecting = false;
  /** Cached preferences path from last successful getPodStructure call */
  private lastKnownPreferencesPath: string | null = null;

  private constructor() {}

  public static getInstance(): SolidPodService {
    if (!SolidPodService.instance) {
      SolidPodService.instance = new SolidPodService();
    }
    return SolidPodService.instance;
  }

  // --- Pod Management ---

  /**
   * Check if the current user has a pod
   */
  public async checkPodExists(): Promise<PodInfo> {
    try {
      const response = await this.fetchWithAuth(`${JSS_BASE_URL}/pods/check`);

      if (!response.ok) {
        throw new Error(`Failed to check pod: ${response.status}`);
      }

      return await response.json();
    } catch (error) {
      logger.error('Failed to check pod existence', { error });
      return { exists: false };
    }
  }

  /**
   * Create a pod for the current user
   */
  public async createPod(name?: string): Promise<PodCreationResult> {
    try {
      const response = await this.fetchWithAuth(`${JSS_BASE_URL}/pods`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name }),
      });

      if (!response.ok) {
        const error = await response.json();
        return {
          success: false,
          error: error.error || 'Pod creation failed',
        };
      }

      const result = await response.json();
      logger.info('Pod created successfully', { podUrl: result.pod_url });

      return {
        success: true,
        podUrl: result.pod_url,
        webId: result.webid,
      };
    } catch (error) {
      logger.error('Failed to create pod', { error });
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
      };
    }
  }

  /**
   * Get the user's pod URL
   */
  public async getPodUrl(): Promise<string | null> {
    const info = await this.checkPodExists();
    return info.podUrl || null;
  }

  /**
   * Initialize pod for the current user (auto-provision if needed)
   * Call this on user login to ensure their pod exists with full structure
   */
  public async initPod(): Promise<PodInitResult> {
    try {
      const response = await this.fetchWithAuth(`${JSS_BASE_URL}/pods/init`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({}),
      });

      if (!response.ok) {
        const error = await response.json().catch(() => ({ error: 'Initialization failed' }));
        return {
          success: false,
          created: false,
          error: error.error || 'Pod initialization failed',
        };
      }

      const result = await response.json();
      logger.info('Pod initialized', {
        podUrl: result.pod_url,
        created: result.created,
      });

      return {
        success: true,
        podUrl: result.pod_url,
        webId: result.webid,
        created: result.created,
        structure: result.structure,
      };
    } catch (error) {
      logger.error('Failed to initialize pod', { error });
      return {
        success: false,
        created: false,
        error: error instanceof Error ? error.message : 'Unknown error',
      };
    }
  }

  /**
   * Ensure user's pod exists, creating if necessary
   * Convenience wrapper that checks and creates in one call
   */
  public async ensurePodExists(): Promise<PodInitResult> {
    return this.initPod();
  }

  /**
   * Get the structure of the user's pod (directory URLs)
   */
  public async getPodStructure(): Promise<PodStructure | null> {
    const result = await this.initPod();
    const structure = result.structure || null;
    if (structure?.preferences) {
      this.lastKnownPreferencesPath = structure.preferences;
    }
    return structure;
  }

  /**
   * Store a preference in the user's preferences container
   */
  public async setPreference(key: string, value: unknown): Promise<boolean> {
    const structure = await this.getPodStructure();
    if (!structure) {
      logger.error('Cannot set preference: pod not initialized');
      return false;
    }

    const safeKey = sanitizePreferenceKey(key);
    const prefPath = `${structure.preferences}${safeKey}.jsonld`;
    return this.putResource(prefPath, {
      '@context': 'https://www.w3.org/ns/ldp',
      '@type': 'Preference',
      key,
      value,
      modified: new Date().toISOString(),
    });
  }

  /**
   * Get a preference from the user's preferences container
   */
  public async getPreference(key: string): Promise<unknown | null> {
    const structure = await this.getPodStructure();
    if (!structure) {
      logger.error('Cannot get preference: pod not initialized');
      return null;
    }

    try {
      const safeKey = sanitizePreferenceKey(key);
      const prefPath = `${structure.preferences}${safeKey}.jsonld`;
      const doc = await this.fetchJsonLd(prefPath);
      return (doc as { value?: unknown }).value ?? null;
    } catch {
      return null;
    }
  }

  // ==========================================================================
  // Graph View Management — Pod-backed named views with cross-device sync
  // ==========================================================================

  /**
   * Save a named graph view to the user's Pod.
   * Stores camera, filters, physics, cluster settings as JSON-LD.
   */
  public async saveGraphView(name: string, viewData: {
    camera?: { x: number; y: number; z: number; fov?: number };
    filters?: Record<string, unknown>;
    physics?: Record<string, unknown>;
    clusters?: Record<string, unknown>;
    pinnedNodes?: number[];
    nodeTypeVisibility?: Record<string, boolean>;
  }): Promise<boolean> {
    const structure = await this.getPodStructure();
    if (!structure) {
      logger.error('Cannot save graph view: pod not initialized');
      return false;
    }

    const safeName = sanitizePreferenceKey(name);
    const viewPath = `${structure.preferences}graph-views/${safeName}.jsonld`;
    const doc = {
      '@context': 'https://schema.org',
      '@type': 'ViewAction',
      '@id': `#${safeName}`,
      name,
      dateCreated: new Date().toISOString(),
      ...viewData,
    };

    const success = await this.putResource(viewPath, doc);
    if (success) {
      logger.info(`Graph view "${name}" saved to Pod`);
    }
    return success;
  }

  /**
   * Load a named graph view from the user's Pod.
   */
  public async loadGraphView(name: string): Promise<Record<string, unknown> | null> {
    const structure = await this.getPodStructure();
    if (!structure) return null;

    try {
      const safeName = sanitizePreferenceKey(name);
      const viewPath = `${structure.preferences}graph-views/${safeName}.jsonld`;
      const doc = await this.fetchJsonLd(viewPath);
      return doc as Record<string, unknown>;
    } catch {
      logger.warn(`Graph view "${name}" not found in Pod`);
      return null;
    }
  }

  /**
   * List all saved graph views from the user's Pod.
   */
  public async listGraphViews(): Promise<string[]> {
    const structure = await this.getPodStructure();
    if (!structure) return [];

    try {
      const containerPath = `${structure.preferences}graph-views/`;
      const response = await this.fetchWithAuth(`/solid${containerPath}`, {
        headers: { Accept: 'application/ld+json' },
      });
      if (!response.ok) return [];
      const data = await response.json();
      // Extract resource names from LDP container listing
      const contains = data['ldp:contains'] || data['contains'] || [];
      const items = Array.isArray(contains) ? contains : [contains];
      return items
        .map((item: { '@id'?: string; url?: string }) => {
          const url = item['@id'] || item.url || '';
          const match = url.match(/\/([^/]+)\.jsonld$/);
          return match ? decodeURIComponent(match[1]) : '';
        })
        .filter(Boolean);
    } catch {
      return [];
    }
  }

  /**
   * Delete a named graph view from the user's Pod.
   */
  public async deleteGraphView(name: string): Promise<boolean> {
    const structure = await this.getPodStructure();
    if (!structure) return false;

    const safeName = sanitizePreferenceKey(name);
    const viewPath = `${structure.preferences}graph-views/${safeName}.jsonld`;
    return this.deleteResource(viewPath);
  }

  /**
   * Subscribe to graph view changes for cross-device sync.
   * Returns an unsubscribe function.
   */
  public subscribeToGraphViewChanges(
    callback: (viewName: string) => void
  ): () => void {
    // Use last-known structure synchronously (subscribe is called from useEffect)
    // The caller should ensure the pod is initialized before subscribing.
    const prefPath = this.lastKnownPreferencesPath;
    if (!prefPath) {
      logger.warn('Cannot subscribe to graph view changes: pod structure not cached');
      return () => {};
    }
    const structure = { preferences: prefPath } as PodStructure;
    // eslint-disable-next-line @typescript-eslint/no-unused-vars

    const containerPath = `${structure.preferences}graph-views/`;
    return this.subscribeToChanges(containerPath, (url) => {
      const match = url.match(/\/([^/]+)\.jsonld$/);
      if (match) {
        callback(decodeURIComponent(match[1]));
      }
    });
  }

  /**
   * Submit an ontology contribution to the user's pod
   */
  public async submitOntologyContribution(contribution: {
    title: string;
    description: string;
    changes: unknown;
  }): Promise<string | null> {
    const structure = await this.getPodStructure();
    if (!structure) {
      logger.error('Cannot submit contribution: pod not initialized');
      return null;
    }

    const slug = `contrib-${Date.now()}`;
    return this.postResource(structure.ontology_contributions, {
      '@context': {
        '@vocab': 'https://narrativegoldmine.com/ontology#',
        schema: 'https://schema.org/',
      },
      '@type': 'OntologyContribution',
      'schema:name': contribution.title,
      'schema:description': contribution.description,
      changes: contribution.changes,
      status: 'draft',
      'schema:dateCreated': new Date().toISOString(),
    }, slug);
  }

  /**
   * Submit an ontology proposal for review
   */
  public async submitOntologyProposal(proposal: {
    title: string;
    description: string;
    contributionUrl: string;
  }): Promise<string | null> {
    const structure = await this.getPodStructure();
    if (!structure) {
      logger.error('Cannot submit proposal: pod not initialized');
      return null;
    }

    const slug = `proposal-${Date.now()}`;
    return this.postResource(structure.ontology_proposals, {
      '@context': {
        '@vocab': 'https://narrativegoldmine.com/ontology#',
        schema: 'https://schema.org/',
      },
      '@type': 'OntologyProposal',
      'schema:name': proposal.title,
      'schema:description': proposal.description,
      contribution: proposal.contributionUrl,
      status: 'pending',
      'schema:dateCreated': new Date().toISOString(),
    }, slug);
  }

  // ==========================================================================
  // Agent Memory — Pod-backed agent memory with per-agent WAC isolation
  // ==========================================================================

  /**
   * Sanitize an agent ID for use in URL paths.
   * Strips dangerous characters while preserving readability.
   */
  private sanitizeAgentId(agentId: string): string {
    let sanitized = agentId.replace(/[\/\\\.]{2,}/g, '');
    sanitized = sanitized.replace(/[\/\\]/g, '-');
    sanitized = sanitized.replace(/^[.\-]+/, '');
    if (!sanitized) {
      throw new Error('Invalid agent ID');
    }
    return sanitized;
  }

  /**
   * Resolve the Pod-relative path for an agent's memory container.
   * Layout: /agents/{agentId}/memory/
   */
  private async resolveAgentMemoryContainer(agentId: string): Promise<string | null> {
    const podUrl = await this.getPodUrl();
    if (!podUrl) {
      logger.error('Cannot resolve agent memory container: no pod');
      return null;
    }

    const safeId = this.sanitizeAgentId(agentId);
    // Extract path portion from podUrl (strip origin if absolute)
    const podPath = podUrl.replace(/^https?:\/\/[^/]+/, '');
    return `${podPath}/agents/${safeId}/memory/`;
  }

  /**
   * Ensure the agent's memory container exists by issuing a PUT
   * on the container path with Link: <ldp:BasicContainer>.
   */
  private async ensureAgentContainer(agentId: string): Promise<string | null> {
    const containerPath = await this.resolveAgentMemoryContainer(agentId);
    if (!containerPath) return null;

    // Create the parent /agents/{id}/ container first, then /memory/
    const safeId = this.sanitizeAgentId(agentId);
    const podUrl = await this.getPodUrl();
    if (!podUrl) return null;
    const podPath = podUrl.replace(/^https?:\/\/[^/]+/, '');

    const agentContainerPath = `${podPath}/agents/${safeId}/`;
    for (const path of [agentContainerPath, containerPath]) {
      const url = this.resolvePath(path);
      const exists = await this.resourceExists(path);
      if (!exists) {
        const response = await this.fetchWithAuth(url, {
          method: 'PUT',
          headers: {
            'Content-Type': 'text/turtle',
            'Link': '<http://www.w3.org/ns/ldp#BasicContainer>; rel="type"',
          },
          body: '',
        });
        if (!response.ok && response.status !== 409) {
          logger.error('Failed to create agent container', { path, status: response.status });
          return null;
        }
      }
    }

    return containerPath;
  }

  /**
   * Store an agent memory entry in the user's Pod.
   *
   * Memory entries are stored as JSON-LD documents at:
   *   /agents/{agentId}/memory/{key}.jsonld
   *
   * @param agentId - Unique identifier for the agent
   * @param entry - Memory entry with key, value, namespace, optional tags and timestamp
   * @returns true if stored successfully
   */
  public async storeAgentMemory(agentId: string, entry: {
    key: string;
    value: string;
    namespace: string;
    tags?: string[];
    timestamp?: string;
  }): Promise<boolean> {
    const containerPath = await this.ensureAgentContainer(agentId);
    if (!containerPath) return false;

    const safeKey = sanitizePreferenceKey(entry.key);
    const resourcePath = `${containerPath}${safeKey}.jsonld`;

    const now = entry.timestamp || new Date().toISOString();
    const npub = nostrAuth.getCurrentUser()?.npub;

    const doc: JsonLdDocument = {
      '@context': 'https://schema.org',
      '@type': 'DigitalDocument',
      identifier: entry.key,
      name: entry.key,
      text: entry.value,
      keywords: entry.tags || [],
      dateCreated: now,
      dateModified: new Date().toISOString(),
      author: npub ? { '@id': `did:nostr:${npub}` } : undefined,
      additionalProperty: {
        '@type': 'PropertyValue',
        name: 'namespace',
        value: entry.namespace,
      },
    };

    const success = await this.putResource(resourcePath, doc);
    if (success) {
      logger.info('Agent memory stored in Pod', { agentId, key: entry.key, namespace: entry.namespace });
    }
    return success;
  }

  /**
   * List agent memory entries from the Pod.
   *
   * Reads the LDP container at /agents/{agentId}/memory/ and returns
   * summary objects for each contained resource.
   *
   * @param agentId - Agent whose memories to list
   * @returns Array of memory entry summaries (key, value, namespace)
   */
  public async listAgentMemories(agentId: string): Promise<Array<{key: string; value: string; namespace: string}>> {
    const containerPath = await this.resolveAgentMemoryContainer(agentId);
    if (!containerPath) return [];

    try {
      const url = this.resolvePath(containerPath);
      const response = await this.fetchWithAuth(url, {
        headers: { Accept: 'application/ld+json' },
      });

      if (!response.ok) return [];

      const data = await response.json();
      const contains = data['ldp:contains'] || data['contains'] || [];
      const items: Array<{ '@id'?: string; url?: string }> = Array.isArray(contains) ? contains : [contains];

      const results: Array<{key: string; value: string; namespace: string}> = [];

      for (const item of items) {
        const itemUrl = item['@id'] || item.url || '';
        const match = itemUrl.match(/\/([^/]+)\.jsonld$/);
        if (!match) continue;

        const key = decodeURIComponent(match[1]);
        try {
          const doc = await this.fetchJsonLd(itemUrl.startsWith('http') ? itemUrl : `${containerPath}${match[1]}.jsonld`);
          results.push({
            key: (doc as { identifier?: string }).identifier || key,
            value: (doc as { text?: string }).text || '',
            namespace: ((doc as { additionalProperty?: { value?: string } }).additionalProperty?.value) || '',
          });
        } catch {
          // Include partial entry if we can't fetch the full document
          results.push({ key, value: '', namespace: '' });
        }
      }

      return results;
    } catch (error) {
      logger.error('Failed to list agent memories', { agentId, error });
      return [];
    }
  }

  /**
   * Get a specific agent memory entry from the Pod.
   *
   * @param agentId - Agent whose memory to retrieve
   * @param key - Memory entry key
   * @returns The full JSON-LD document or null if not found
   */
  public async getAgentMemory(agentId: string, key: string): Promise<Record<string, unknown> | null> {
    const containerPath = await this.resolveAgentMemoryContainer(agentId);
    if (!containerPath) return null;

    try {
      const safeKey = sanitizePreferenceKey(key);
      const resourcePath = `${containerPath}${safeKey}.jsonld`;
      const doc = await this.fetchJsonLd(resourcePath);
      return doc as Record<string, unknown>;
    } catch {
      logger.debug('Agent memory not found', { agentId, key });
      return null;
    }
  }

  /**
   * Delete an agent memory entry from the Pod.
   * This allows users to revoke specific agent memories.
   *
   * @param agentId - Agent whose memory to delete
   * @param key - Memory entry key to remove
   * @returns true if deleted (or already absent)
   */
  public async deleteAgentMemory(agentId: string, key: string): Promise<boolean> {
    const containerPath = await this.resolveAgentMemoryContainer(agentId);
    if (!containerPath) return false;

    const safeKey = sanitizePreferenceKey(key);
    const resourcePath = `${containerPath}${safeKey}.jsonld`;
    return this.deleteResource(resourcePath);
  }

  /**
   * Set WAC (Web Access Control) permissions for an agent's memory container.
   *
   * Creates/updates the ACL resource for the agent's memory container,
   * granting the specified modes to the agent's WebID while preserving
   * the owner's full control.
   *
   * @param agentId - Agent whose container to configure
   * @param permissions - Agent WebID and access modes to grant
   * @returns true if ACL was set successfully
   */
  public async setAgentMemoryAccess(agentId: string, permissions: {
    agentWebId: string;
    modes: ('Read' | 'Write' | 'Append')[];
  }): Promise<boolean> {
    const containerPath = await this.resolveAgentMemoryContainer(agentId);
    if (!containerPath) return false;

    const resolvedContainer = this.resolvePath(containerPath);

    // Build WAC mode URIs
    const agentModes = permissions.modes
      .map((m) => `acl:${m}`)
      .join(', ');

    // Get the owner's WebID
    const podInfo = await this.checkPodExists();
    const ownerWebId = podInfo.webId || '';

    // ACL in Turtle format following WAC spec
    const aclTurtle = `@prefix acl: <http://www.w3.org/ns/auth/acl#>.
@prefix foaf: <http://xmlns.com/foaf/0.1/>.

# Owner retains full control
<#owner>
    a acl:Authorization;
    acl:agent <${ownerWebId}>;
    acl:accessTo <${resolvedContainer}>;
    acl:default <${resolvedContainer}>;
    acl:mode acl:Read, acl:Write, acl:Control.

# Agent access
<#agent>
    a acl:Authorization;
    acl:agent <${permissions.agentWebId}>;
    acl:accessTo <${resolvedContainer}>;
    acl:default <${resolvedContainer}>;
    acl:mode ${agentModes}.
`;

    const aclPath = `${containerPath}.acl`;
    const aclUrl = this.resolvePath(aclPath);

    const response = await this.fetchWithAuth(aclUrl, {
      method: 'PUT',
      headers: { 'Content-Type': 'text/turtle' },
      body: aclTurtle,
    });

    if (!response.ok) {
      logger.error('Failed to set agent memory ACL', { agentId, status: response.status });
      return false;
    }

    logger.info('Agent memory ACL updated', {
      agentId,
      agentWebId: permissions.agentWebId,
      modes: permissions.modes,
    });
    return true;
  }

  // --- Connection Methods ---

  /**
   * Connect to a user's pod by their npub (Nostr public key in bech32 format)
   * Sets up the pod context for subsequent operations
   * @param npub - Nostr public key in npub format (e.g., npub1...)
   * @returns Pod connection info
   */
  public async connectToPod(npub: string): Promise<PodInfo> {
    logger.info('Connecting to pod for npub', { npub });

    // Validate npub format
    if (!npub.startsWith('npub1')) {
      throw new Error('Invalid npub format. Expected npub1...');
    }

    try {
      const response = await this.fetchWithAuth(`${JSS_BASE_URL}/pods/connect`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ npub }),
      });

      if (!response.ok) {
        const error = await response.json().catch(() => ({ error: 'Connection failed' }));
        throw new Error(error.error || `Pod connection failed: ${response.status}`);
      }

      const info = await response.json();
      logger.info('Connected to pod', { podUrl: info.podUrl });

      // Auto-connect WebSocket if configured
      if (JSS_WS_URL && !this.wsConnection) {
        this.connectWebSocket();
      }

      return {
        exists: true,
        podUrl: info.podUrl || info.pod_url,
        webId: info.webId || info.webid,
      };
    } catch (error) {
      logger.error('Failed to connect to pod', { npub, error });
      throw error;
    }
  }

  // --- LDP Operations ---

  /**
   * Read a resource from the pod as JSON-LD
   * Alias for fetchJsonLd with enhanced error handling
   * @param path - Resource path relative to pod or absolute URL
   * @returns JSON-LD document
   */
  public async readResource(path: string): Promise<JsonLdDocument> {
    return this.fetchJsonLd(path);
  }

  /**
   * Write a resource to the pod
   * Alias for putResource with JSON-LD default and flexible content type
   * @param path - Resource path relative to pod or absolute URL
   * @param content - Content to write (object or string)
   * @returns Success status
   */
  public async writeResource(
    path: string,
    content: JsonLdDocument | Record<string, unknown> | string
  ): Promise<boolean> {
    // Convert plain objects to JSON-LD format
    const jsonLdContent = this.ensureJsonLd(content);
    return this.putResource(path, jsonLdContent);
  }

  /**
   * Subscribe to changes on a resource path
   * Wrapper around subscribe with simplified callback
   * @param path - Resource path to watch
   * @param callback - Function called when resource changes
   * @returns Unsubscribe function
   */
  public subscribeToChanges(
    path: string,
    callback: (url: string, type: 'created' | 'updated' | 'deleted') => void
  ): () => void {
    const resourceUrl = this.resolvePath(path);

    // Ensure WebSocket is connected
    if (!this.wsConnection) {
      this.connectWebSocket();
    }

    return this.subscribe(resourceUrl, (notification) => {
      if (notification.type === 'pub') {
        // Determine change type based on resource state
        callback(notification.url, 'updated');
      }
    });
  }

  /**
   * Fetch a resource as JSON-LD
   */
  public async fetchJsonLd(resourcePath: string): Promise<JsonLdDocument> {
    const url = this.resolvePath(resourcePath);
    const response = await this.fetchWithAuth(url, {
      headers: { Accept: 'application/ld+json' },
    });

    if (!response.ok) {
      throw new Error(`Failed to fetch ${resourcePath}: ${response.status}`);
    }

    return response.json();
  }

  /**
   * Fetch a resource as Turtle (for external tools)
   */
  public async fetchTurtle(resourcePath: string): Promise<string> {
    const url = this.resolvePath(resourcePath);
    const response = await this.fetchWithAuth(url, {
      headers: { Accept: 'text/turtle' },
    });

    if (!response.ok) {
      throw new Error(`Failed to fetch Turtle ${resourcePath}: ${response.status}`);
    }

    return response.text();
  }

  /**
   * Create or update a resource
   */
  public async putResource(
    resourcePath: string,
    data: JsonLdDocument | string,
    contentType: 'application/ld+json' | 'text/turtle' = 'application/ld+json'
  ): Promise<boolean> {
    const url = this.resolvePath(resourcePath);
    const body = typeof data === 'string' ? data : JSON.stringify(data);

    const response = await this.fetchWithAuth(url, {
      method: 'PUT',
      headers: { 'Content-Type': contentType },
      body,
    });

    if (!response.ok) {
      logger.error('PUT failed', { resourcePath, status: response.status });
      return false;
    }

    logger.debug('Resource updated', { resourcePath });
    return true;
  }

  /**
   * Create a resource in a container (POST)
   */
  public async postResource(
    containerPath: string,
    data: JsonLdDocument,
    slug?: string
  ): Promise<string | null> {
    const url = this.resolvePath(containerPath);
    const headers: Record<string, string> = {
      'Content-Type': 'application/ld+json',
    };

    if (slug) {
      headers['Slug'] = slug;
    }

    const response = await this.fetchWithAuth(url, {
      method: 'POST',
      headers,
      body: JSON.stringify(data),
    });

    if (!response.ok) {
      logger.error('POST failed', { containerPath, status: response.status });
      return null;
    }

    // Return the created resource URL from Location header
    return response.headers.get('Location');
  }

  /**
   * Delete a resource
   */
  public async deleteResource(resourcePath: string): Promise<boolean> {
    const url = this.resolvePath(resourcePath);

    const response = await this.fetchWithAuth(url, {
      method: 'DELETE',
    });

    if (!response.ok && response.status !== 404) {
      logger.error('DELETE failed', { resourcePath, status: response.status });
      return false;
    }

    return true;
  }

  /**
   * Check if a resource exists (HEAD)
   */
  public async resourceExists(resourcePath: string): Promise<boolean> {
    const url = this.resolvePath(resourcePath);

    try {
      const response = await this.fetchWithAuth(url, { method: 'HEAD' });
      return response.ok;
    } catch {
      return false;
    }
  }

  // --- WebSocket Notifications ---

  /**
   * Connect to JSS WebSocket for real-time notifications
   */
  public connectWebSocket(): void {
    if (!JSS_WS_URL) {
      logger.warn('JSS WebSocket URL not configured');
      return;
    }

    if (this.wsConnection?.readyState === WebSocket.OPEN) {
      logger.debug('WebSocket already connected');
      return;
    }

    try {
      // Validate WebSocket URL before connecting
      const validatedUrl = new URL(JSS_WS_URL);
      if (validatedUrl.protocol !== 'ws:' && validatedUrl.protocol !== 'wss:') {
        logger.error('Invalid WebSocket protocol', { protocol: validatedUrl.protocol });
        return;
      }
      this.wsConnection = new WebSocket(validatedUrl.href);

      this.wsConnection.onopen = () => {
        logger.info('JSS WebSocket connected');
        this.reconnectAttempts = 0;
        webSocketRegistry.register(REGISTRY_NAME, validatedUrl.href, this.wsConnection!);
        webSocketEventBus.emit('connection:open', { name: REGISTRY_NAME, url: validatedUrl.href });
        // Protocol handshake will be handled in onmessage
      };

      this.wsConnection.onmessage = (event) => {
        const msg = event.data.toString().trim();
        webSocketEventBus.emit('message:pod', { data: msg });
        this.handleWebSocketMessage(msg);
      };

      this.wsConnection.onerror = (error) => {
        logger.error('JSS WebSocket error', { error });
        webSocketEventBus.emit('connection:error', { name: REGISTRY_NAME, error });
      };

      this.wsConnection.onclose = (event) => {
        logger.info('JSS WebSocket disconnected');
        webSocketRegistry.unregister(REGISTRY_NAME);
        webSocketEventBus.emit('connection:close', {
          name: REGISTRY_NAME,
          code: event.code,
          reason: event.reason,
        });
        if (this.isDisconnecting) {
          this.isDisconnecting = false;
          return;
        }
        this.handleReconnect();
      };
    } catch (error) {
      logger.error('Failed to connect WebSocket', { error });
    }
  }

  /**
   * Subscribe to notifications for a resource
   */
  public subscribe(resourceUrl: string, callback: NotificationCallback): () => void {
    if (!this.subscriptions.has(resourceUrl)) {
      this.subscriptions.set(resourceUrl, new Set());

      // Send subscription if connected
      if (this.wsConnection?.readyState === WebSocket.OPEN) {
        this.wsConnection.send(`sub ${resourceUrl}`);
      }
    }

    this.subscriptions.get(resourceUrl)!.add(callback);

    // Return unsubscribe function
    return () => {
      this.subscriptions.get(resourceUrl)?.delete(callback);

      if (this.subscriptions.get(resourceUrl)?.size === 0) {
        if (this.wsConnection?.readyState === WebSocket.OPEN) {
          this.wsConnection.send(`unsub ${resourceUrl}`);
        }
        this.subscriptions.delete(resourceUrl);
      }
    };
  }

  /**
   * Disconnect WebSocket
   */
  public disconnect(): void {
    if (this.reconnectTimerId !== null) {
      clearTimeout(this.reconnectTimerId);
      this.reconnectTimerId = null;
    }
    this.isDisconnecting = true;
    webSocketRegistry.unregister(REGISTRY_NAME);
    if (this.wsConnection) {
      this.wsConnection.close();
      this.wsConnection = null;
    }
    this.subscriptions.clear();
  }

  // ==========================================================================
  // Type Index — Agent and view discovery (Phase 3)
  // ==========================================================================

  /**
   * Solid Type Index namespace constants
   */
  private static readonly SOLID_NS = 'http://www.w3.org/ns/solid/terms#';
  private static readonly SCHEMA_NS = 'https://schema.org/';
  private static readonly VISIONFLOW_NS = 'https://narrativegoldmine.com/ontology#';
  private static readonly TYPE_INDEX_PATH = '/settings/publicTypeIndex.jsonld';

  /**
   * Get or create the public Type Index document for the current user.
   * The Type Index is stored at /settings/publicTypeIndex.jsonld and linked
   * from the user's WebID profile via solid:publicTypeIndex.
   *
   * @returns The URL of the public Type Index document
   * @throws Error if the pod is not initialized
   */
  public async ensurePublicTypeIndex(): Promise<string> {
    const structure = await this.getPodStructure();
    if (!structure) {
      throw new Error('Cannot ensure Type Index: pod not initialized');
    }

    // Derive the type index path relative to the pod's preferences container
    // preferences path is like /pods/<id>/settings/preferences/
    // We want /pods/<id>/settings/publicTypeIndex.jsonld
    const prefPath = structure.preferences;
    const settingsBase = prefPath.substring(0, prefPath.indexOf('/settings/') + '/settings/'.length);
    const typeIndexPath = `${settingsBase}publicTypeIndex.jsonld`;

    const exists = await this.resourceExists(typeIndexPath);
    if (exists) {
      return this.resolvePath(typeIndexPath);
    }

    // Create empty Type Index document
    const typeIndexDoc: JsonLdDocument = {
      '@context': {
        'solid': SolidPodService.SOLID_NS,
        'schema': SolidPodService.SCHEMA_NS,
        'vf': SolidPodService.VISIONFLOW_NS,
      },
      '@type': 'solid:TypeIndex',
      'solid:typeRegistration': [],
    };

    const created = await this.putResource(typeIndexPath, typeIndexDoc);
    if (!created) {
      throw new Error('Failed to create public Type Index document');
    }

    // Link from profile (best-effort — profile may already have this triple)
    await this.linkTypeIndexFromProfile(typeIndexPath);

    logger.info('Public Type Index created', { path: typeIndexPath });
    return this.resolvePath(typeIndexPath);
  }

  /**
   * Register a graph view in the public Type Index so other users can
   * discover shared views via their peer's WebID.
   *
   * @param viewName - Human-readable name for the view
   * @param viewUrl - Full URL or pod-relative path to the view resource
   * @returns true if registration succeeded
   */
  public async registerViewInTypeIndex(
    viewName: string,
    viewUrl: string
  ): Promise<boolean> {
    try {
      const typeIndexUrl = await this.ensurePublicTypeIndex();
      const typeIndexPath = this.extractPath(typeIndexUrl);
      const doc = await this.fetchJsonLd(typeIndexPath);

      const registrations = this.extractRegistrations(doc);

      // Avoid duplicate registration for the same view URL
      const resolvedViewUrl = this.resolvePath(viewUrl);
      const alreadyRegistered = registrations.some(
        (r) => r['solid:instance'] === resolvedViewUrl
      );
      if (alreadyRegistered) {
        logger.debug('View already registered in Type Index', { viewName });
        return true;
      }

      const newRegistration: TypeRegistration = {
        '@type': 'solid:TypeRegistration',
        'solid:forClass': 'schema:ViewAction',
        'solid:instance': resolvedViewUrl,
        'vf:label': viewName,
        'vf:registeredAt': new Date().toISOString(),
      };

      registrations.push(newRegistration);
      (doc as Record<string, unknown>)['solid:typeRegistration'] = registrations;

      const success = await this.putResource(typeIndexPath, doc);
      if (success) {
        logger.info('View registered in Type Index', { viewName, viewUrl });
      }
      return success;
    } catch (error) {
      logger.error('Failed to register view in Type Index', { viewName, error });
      return false;
    }
  }

  /**
   * Register agent capabilities in the public Type Index so other users
   * can discover available agents for collaboration.
   *
   * @param agentId - Unique identifier for the agent
   * @param capabilities - List of capability strings (e.g., ["reasoning", "code-review"])
   * @returns true if registration succeeded
   */
  public async registerAgentInTypeIndex(
    agentId: string,
    capabilities: string[]
  ): Promise<boolean> {
    try {
      const typeIndexUrl = await this.ensurePublicTypeIndex();
      const typeIndexPath = this.extractPath(typeIndexUrl);
      const doc = await this.fetchJsonLd(typeIndexPath);

      const registrations = this.extractRegistrations(doc);

      // Remove any existing registration for this agent (update semantics)
      const filtered = registrations.filter(
        (r) => !(r['solid:forClass'] === 'vf:Agent' && r['vf:agentId'] === agentId)
      );

      const newRegistration: TypeRegistration = {
        '@type': 'solid:TypeRegistration',
        'solid:forClass': 'vf:Agent',
        'vf:agentId': agentId,
        'vf:capabilities': capabilities,
        'vf:registeredAt': new Date().toISOString(),
      };

      filtered.push(newRegistration);
      (doc as Record<string, unknown>)['solid:typeRegistration'] = filtered;

      const success = await this.putResource(typeIndexPath, doc);
      if (success) {
        logger.info('Agent registered in Type Index', { agentId, capabilities });
      }
      return success;
    } catch (error) {
      logger.error('Failed to register agent in Type Index', { agentId, error });
      return false;
    }
  }

  /**
   * Discover shared views from another user's public Type Index.
   * Fetches the remote user's WebID profile, resolves their publicTypeIndex,
   * and extracts all ViewAction registrations.
   *
   * @param webId - The remote user's WebID URL
   * @returns Array of discovered views with name and URL
   */
  public async discoverSharedViews(webId: string): Promise<DiscoveredView[]> {
    try {
      const typeIndexUrl = await this.resolveRemoteTypeIndex(webId);
      if (!typeIndexUrl) {
        logger.debug('No public Type Index found for WebID', { webId });
        return [];
      }

      const doc = await this.fetchJsonLd(typeIndexUrl);
      const registrations = this.extractRegistrations(doc);

      return registrations
        .filter((r) => r['solid:forClass'] === 'schema:ViewAction')
        .map((r) => ({
          name: (r['vf:label'] as string) || this.extractViewName(r['solid:instance'] as string),
          url: r['solid:instance'] as string,
        }))
        .filter((v) => v.url);
    } catch (error) {
      logger.error('Failed to discover shared views', { webId, error });
      return [];
    }
  }

  /**
   * Discover available agents from another user's public Type Index.
   * Fetches the remote user's WebID profile, resolves their publicTypeIndex,
   * and extracts all Agent registrations with capabilities.
   *
   * @param webId - The remote user's WebID URL
   * @returns Array of discovered agents with id and capabilities
   */
  public async discoverAgents(webId: string): Promise<DiscoveredAgent[]> {
    try {
      const typeIndexUrl = await this.resolveRemoteTypeIndex(webId);
      if (!typeIndexUrl) {
        logger.debug('No public Type Index found for WebID', { webId });
        return [];
      }

      const doc = await this.fetchJsonLd(typeIndexUrl);
      const registrations = this.extractRegistrations(doc);

      return registrations
        .filter((r) => r['solid:forClass'] === 'vf:Agent')
        .map((r) => ({
          id: (r['vf:agentId'] as string) || '',
          capabilities: this.normalizeCapabilities(r['vf:capabilities']),
        }))
        .filter((a) => a.id);
    } catch (error) {
      logger.error('Failed to discover agents', { webId, error });
      return [];
    }
  }

  // --- Type Index Private Helpers ---

  /**
   * Extract type registrations array from a Type Index document,
   * handling both array and single-object forms.
   */
  private extractRegistrations(doc: JsonLdDocument): TypeRegistration[] {
    const raw = doc['solid:typeRegistration'];
    if (!raw) return [];
    if (Array.isArray(raw)) return raw as TypeRegistration[];
    return [raw as TypeRegistration];
  }

  /**
   * Resolve a remote user's public Type Index URL from their WebID profile.
   * Fetches the WebID document and looks for solid:publicTypeIndex.
   */
  private async resolveRemoteTypeIndex(webId: string): Promise<string | null> {
    try {
      const response = await this.fetchWithAuth(webId, {
        headers: { Accept: 'application/ld+json' },
      });

      if (!response.ok) {
        logger.warn('Failed to fetch WebID profile', { webId, status: response.status });
        return null;
      }

      const profile = await response.json();

      // Look for solid:publicTypeIndex in profile
      const typeIndexRef =
        profile['solid:publicTypeIndex'] ||
        profile['http://www.w3.org/ns/solid/terms#publicTypeIndex'];

      if (!typeIndexRef) return null;

      // Handle both string URL and object with @id
      if (typeof typeIndexRef === 'string') return typeIndexRef;
      if (typeIndexRef['@id']) return typeIndexRef['@id'];

      return null;
    } catch (error) {
      logger.error('Failed to resolve remote Type Index', { webId, error });
      return null;
    }
  }

  /**
   * Link the Type Index document from the user's WebID profile.
   * Reads the profile, adds solid:publicTypeIndex if missing, writes it back.
   */
  private async linkTypeIndexFromProfile(typeIndexPath: string): Promise<void> {
    try {
      const podInfo = await this.checkPodExists();
      if (!podInfo.webId) return;

      // Fetch current profile
      const profilePath = podInfo.webId;
      let profile: JsonLdDocument;
      try {
        profile = await this.fetchJsonLd(profilePath);
      } catch {
        // Profile doesn't exist or isn't JSON-LD — skip linking
        logger.debug('Cannot link Type Index: profile not accessible');
        return;
      }

      // Already linked
      if (profile['solid:publicTypeIndex']) return;

      // Add the link
      const resolvedTypeIndexUrl = this.resolvePath(typeIndexPath);
      (profile as Record<string, unknown>)['solid:publicTypeIndex'] = {
        '@id': resolvedTypeIndexUrl,
      };

      await this.putResource(profilePath, profile);
      logger.debug('Type Index linked from profile', { profilePath });
    } catch (error) {
      logger.warn('Failed to link Type Index from profile (non-fatal)', { error });
    }
  }

  /**
   * Extract a view name from a view URL (fallback when no label stored).
   */
  private extractViewName(url: string): string {
    if (!url) return '';
    const match = url.match(/\/([^/]+?)(?:\.jsonld)?$/);
    return match ? decodeURIComponent(match[1]) : url;
  }

  /**
   * Normalize capabilities from stored format (may be string, array, or comma-separated).
   */
  private normalizeCapabilities(raw: unknown): string[] {
    if (!raw) return [];
    if (Array.isArray(raw)) return raw.map(String);
    if (typeof raw === 'string') return raw.split(',').map((s) => s.trim()).filter(Boolean);
    return [];
  }

  /**
   * Extract the path portion from a full URL for use with internal methods.
   */
  private extractPath(url: string): string {
    try {
      const parsed = new URL(url);
      return parsed.pathname;
    } catch {
      // Already a path
      return url;
    }
  }

  // --- Private Methods ---

  /**
   * Ensure content is in JSON-LD format
   */
  private ensureJsonLd(
    content: JsonLdDocument | Record<string, unknown> | string
  ): JsonLdDocument {
    if (typeof content === 'string') {
      try {
        const parsed = JSON.parse(content);
        return this.ensureJsonLd(parsed);
      } catch {
        // Treat as plain text content
        return {
          '@context': 'https://www.w3.org/ns/ldp',
          '@type': 'Resource',
          content: content,
        };
      }
    }

    // Already has @context - return as-is
    if ('@context' in content && content['@context']) {
      return content as JsonLdDocument;
    }

    // Wrap plain object in JSON-LD structure
    const { '@context': _, '@type': __, ...rest } = content as Record<string, unknown>;
    return {
      '@context': {
        '@vocab': 'https://narrativegoldmine.com/ontology#',
        ldp: 'https://www.w3.org/ns/ldp#',
        xsd: 'http://www.w3.org/2001/XMLSchema#',
      },
      '@type': 'Resource',
      ...rest,
    };
  }

  private async fetchWithAuth(
    url: string,
    options: RequestInit = {}
  ): Promise<Response> {
    const headers = new Headers(options.headers);

    if (nostrAuth.isAuthenticated()) {
      if (nostrAuth.isDevMode()) {
        headers.set('Authorization', 'Bearer dev-session-token');
        const user = nostrAuth.getCurrentUser();
        if (user?.pubkey) headers.set('X-Nostr-Pubkey', user.pubkey);
      } else {
        // Always sign with NIP-98 ourselves. NIP-07 extensions may also
        // intercept, but their retry-on-401 is unreliable for mutations.
        try {
          const method = (options.method || 'GET').toUpperCase();
          const body = typeof options.body === 'string' ? options.body : undefined;
          const absoluteUrl = url.startsWith('http') ? url : `${window.location.origin}${url}`;
          const token = await nostrAuth.signRequest(absoluteUrl, method, body);
          headers.set('Authorization', `Nostr ${token}`);
        } catch (e) {
          logger.warn('NIP-98 signing failed:', e);
        }
      }
    } else if (nostrAuth.getCurrentUser()) {
      // User is "logged in" (localStorage) but has no signing capability —
      // stale session after page reload where private key wasn't persisted.
      logger.warn(
        'Stale auth session: user exists but cannot sign requests. ' +
        'Please log out and log back in.'
      );
    }

    return fetch(url, {
      ...options,
      headers,
      credentials: 'include',
    });
  }

  private resolvePath(path: string): string {
    if (path.startsWith('http://') || path.startsWith('https://')) {
      // Rewrite internal JSS URLs to use the proxy path so the browser
      // doesn't try to reach the Docker-internal hostname directly.
      const jssPattern = /^https?:\/\/[^/]*(?:visionflow-jss|jss|localhost)[^/]*(?::\d+)?\/(.*)$/;
      const match = path.match(jssPattern);
      if (match) {
        return `${JSS_BASE_URL}/${match[1]}`;
      }
      return path;
    }

    // Already resolved to proxy path — don't double-prefix
    if (path.startsWith(JSS_BASE_URL + '/') || path === JSS_BASE_URL) {
      return path;
    }

    // Remove leading slash if present
    const cleanPath = path.startsWith('/') ? path.slice(1) : path;
    return `${JSS_BASE_URL}/${cleanPath}`;
  }

  private handleWebSocketMessage(msg: string): void {
    if (msg.startsWith('protocol ')) {
      // Handshake complete, resubscribe to all resources
      logger.debug('WebSocket protocol handshake complete');
      for (const url of this.subscriptions.keys()) {
        this.wsConnection?.send(`sub ${url}`);
      }
    } else if (msg.startsWith('ack ')) {
      const url = msg.slice(4);
      logger.debug('Subscription acknowledged', { url });
      this.notifySubscribers(url, { type: 'ack', url });
    } else if (msg.startsWith('pub ')) {
      const url = msg.slice(4);
      logger.debug('Resource changed', { url });
      this.notifySubscribers(url, { type: 'pub', url });
    }
  }

  private notifySubscribers(url: string, notification: SolidNotification): void {
    // Notify exact URL subscribers
    const callbacks = this.subscriptions.get(url);
    callbacks?.forEach((cb) => cb(notification));

    // Also notify container subscribers (parent directory)
    const containerUrl = url.substring(0, url.lastIndexOf('/') + 1);
    if (containerUrl !== url) {
      const containerCallbacks = this.subscriptions.get(containerUrl);
      containerCallbacks?.forEach((cb) => cb(notification));
    }
  }

  private handleReconnect(): void {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      logger.warn('Max reconnect attempts reached');
      return;
    }

    this.reconnectAttempts++;
    const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);

    logger.info(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);

    this.reconnectTimerId = setTimeout(() => {
      this.reconnectTimerId = null;
      this.connectWebSocket();
    }, delay);
  }
}

// Export singleton instance
const solidPodService = SolidPodService.getInstance();
export default solidPodService;
