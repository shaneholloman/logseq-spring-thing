/**
 * Solid Type Index
 *
 * Implements the Solid Type Index specification (Phase 3 discovery):
 * - Ensure/create the public Type Index document
 * - Register and look up graph views
 * - Register and look up agents with capabilities
 * - Resolve remote users' Type Indexes via their WebID profile
 */

import { createLogger } from '../../utils/loggerConfig';
import {
  JsonLdDocument,
  fetchJsonLd,
  putResource,
  resourceExists,
  resolvePath,
  extractPath,
  fetchWithAuth,
} from './ldpClient';

const logger = createLogger('SolidPodService:typeIndex');

// ---------------------------------------------------------------------------
// Namespaces
// ---------------------------------------------------------------------------

const SOLID_NS = 'http://www.w3.org/ns/solid/terms#';
const SCHEMA_NS = 'https://schema.org/';
const VISIONFLOW_NS = 'https://narrativegoldmine.com/ontology#';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Get or create the public Type Index document for the given pod structure.
 *
 * @param preferencesPath - The pod's preferences container path
 * @returns The absolute URL of the public Type Index document
 */
export async function ensurePublicTypeIndex(preferencesPath: string): Promise<string> {
  const settingsBase = preferencesPath.substring(
    0,
    preferencesPath.indexOf('/settings/') + '/settings/'.length
  );
  const typeIndexPath = `${settingsBase}publicTypeIndex.jsonld`;

  const exists = await resourceExists(typeIndexPath);
  if (exists) {
    return resolvePath(typeIndexPath);
  }

  const typeIndexDoc: JsonLdDocument = {
    '@context': {
      solid: SOLID_NS,
      schema: SCHEMA_NS,
      vf: VISIONFLOW_NS,
    },
    '@type': 'solid:TypeIndex',
    'solid:typeRegistration': [],
  };

  const created = await putResource(typeIndexPath, typeIndexDoc);
  if (!created) {
    throw new Error('Failed to create public Type Index document');
  }

  logger.info('Public Type Index created', { path: typeIndexPath });
  return resolvePath(typeIndexPath);
}

/**
 * Link the Type Index document from the user's WebID profile.
 * Best-effort — non-fatal if the profile is inaccessible.
 */
export async function linkTypeIndexFromProfile(
  typeIndexPath: string,
  webId: string
): Promise<void> {
  try {
    let profile: JsonLdDocument;
    try {
      profile = await fetchJsonLd(webId);
    } catch {
      logger.debug('Cannot link Type Index: profile not accessible');
      return;
    }

    if (profile['solid:publicTypeIndex']) return;

    const resolvedTypeIndexUrl = resolvePath(typeIndexPath);
    (profile as Record<string, unknown>)['solid:publicTypeIndex'] = {
      '@id': resolvedTypeIndexUrl,
    };

    await putResource(webId, profile);
    logger.debug('Type Index linked from profile', { profilePath: webId });
  } catch (error) {
    logger.warn('Failed to link Type Index from profile (non-fatal)', { error });
  }
}

/**
 * Register a graph view in the public Type Index.
 *
 * @param preferencesPath - The pod's preferences container path
 * @param viewName - Human-readable label
 * @param viewUrl - Pod-relative or absolute URL to the view resource
 * @returns true if registration succeeded
 */
export async function registerViewInTypeIndex(
  preferencesPath: string,
  viewName: string,
  viewUrl: string
): Promise<boolean> {
  try {
    const typeIndexUrl = await ensurePublicTypeIndex(preferencesPath);
    const typeIndexPath = extractPath(typeIndexUrl);
    const doc = await fetchJsonLd(typeIndexPath);
    const registrations = extractRegistrations(doc);

    const resolvedViewUrl = resolvePath(viewUrl);
    const alreadyRegistered = registrations.some(
      (r) => r['solid:instance'] === resolvedViewUrl
    );
    if (alreadyRegistered) {
      logger.debug('View already registered in Type Index', { viewName });
      return true;
    }

    const newReg: TypeRegistration = {
      '@type': 'solid:TypeRegistration',
      'solid:forClass': 'schema:ViewAction',
      'solid:instance': resolvedViewUrl,
      'vf:label': viewName,
      'vf:registeredAt': new Date().toISOString(),
    };

    registrations.push(newReg);
    (doc as Record<string, unknown>)['solid:typeRegistration'] = registrations;

    const success = await putResource(typeIndexPath, doc);
    if (success) logger.info('View registered in Type Index', { viewName, viewUrl });
    return success;
  } catch (error) {
    logger.error('Failed to register view in Type Index', { viewName, error });
    return false;
  }
}

/**
 * Register agent capabilities in the public Type Index.
 *
 * @param preferencesPath - The pod's preferences container path
 * @param agentId - Unique identifier for the agent
 * @param capabilities - List of capability strings
 * @returns true if registration succeeded
 */
export async function registerAgentInTypeIndex(
  preferencesPath: string,
  agentId: string,
  capabilities: string[]
): Promise<boolean> {
  try {
    const typeIndexUrl = await ensurePublicTypeIndex(preferencesPath);
    const typeIndexPath = extractPath(typeIndexUrl);
    const doc = await fetchJsonLd(typeIndexPath);
    const registrations = extractRegistrations(doc);

    // Update semantics: remove existing entry for this agent
    const filtered = registrations.filter(
      (r) => !(r['solid:forClass'] === 'vf:Agent' && r['vf:agentId'] === agentId)
    );

    const newReg: TypeRegistration = {
      '@type': 'solid:TypeRegistration',
      'solid:forClass': 'vf:Agent',
      'vf:agentId': agentId,
      'vf:capabilities': capabilities,
      'vf:registeredAt': new Date().toISOString(),
    };

    filtered.push(newReg);
    (doc as Record<string, unknown>)['solid:typeRegistration'] = filtered;

    const success = await putResource(typeIndexPath, doc);
    if (success) logger.info('Agent registered in Type Index', { agentId, capabilities });
    return success;
  } catch (error) {
    logger.error('Failed to register agent in Type Index', { agentId, error });
    return false;
  }
}

/**
 * Discover shared views from a remote user's public Type Index.
 *
 * @param webId - The remote user's WebID URL
 */
export async function discoverSharedViews(webId: string): Promise<DiscoveredView[]> {
  try {
    const typeIndexUrl = await resolveRemoteTypeIndex(webId);
    if (!typeIndexUrl) {
      logger.debug('No public Type Index found for WebID', { webId });
      return [];
    }

    const doc = await fetchJsonLd(typeIndexUrl);
    const registrations = extractRegistrations(doc);

    return registrations
      .filter((r) => r['solid:forClass'] === 'schema:ViewAction')
      .map((r) => ({
        name:
          (r['vf:label'] as string) || extractViewName(r['solid:instance'] as string),
        url: r['solid:instance'] as string,
      }))
      .filter((v) => Boolean(v.url));
  } catch (error) {
    logger.error('Failed to discover shared views', { webId, error });
    return [];
  }
}

/**
 * Discover available agents from a remote user's public Type Index.
 *
 * @param webId - The remote user's WebID URL
 */
export async function discoverAgents(webId: string): Promise<DiscoveredAgent[]> {
  try {
    const typeIndexUrl = await resolveRemoteTypeIndex(webId);
    if (!typeIndexUrl) {
      logger.debug('No public Type Index found for WebID', { webId });
      return [];
    }

    const doc = await fetchJsonLd(typeIndexUrl);
    const registrations = extractRegistrations(doc);

    return registrations
      .filter((r) => r['solid:forClass'] === 'vf:Agent')
      .map((r) => ({
        id: (r['vf:agentId'] as string) || '',
        capabilities: normalizeCapabilities(r['vf:capabilities']),
      }))
      .filter((a) => Boolean(a.id));
  } catch (error) {
    logger.error('Failed to discover agents', { webId, error });
    return [];
  }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

function extractRegistrations(doc: JsonLdDocument): TypeRegistration[] {
  const raw = doc['solid:typeRegistration'];
  if (!raw) return [];
  if (Array.isArray(raw)) return raw as TypeRegistration[];
  return [raw as TypeRegistration];
}

async function resolveRemoteTypeIndex(webId: string): Promise<string | null> {
  try {
    const response = await fetchWithAuth(webId, {
      headers: { Accept: 'application/ld+json' },
    });

    if (!response.ok) {
      logger.warn('Failed to fetch WebID profile', { webId, status: response.status });
      return null;
    }

    const profile = await response.json();
    const typeIndexRef =
      profile['solid:publicTypeIndex'] ||
      profile['http://www.w3.org/ns/solid/terms#publicTypeIndex'];

    if (!typeIndexRef) return null;
    if (typeof typeIndexRef === 'string') return typeIndexRef;
    if (typeIndexRef['@id']) return typeIndexRef['@id'] as string;
    return null;
  } catch (error) {
    logger.error('Failed to resolve remote Type Index', { webId, error });
    return null;
  }
}

function extractViewName(url: string): string {
  if (!url) return '';
  const match = url.match(/\/([^/]+?)(?:\.jsonld)?$/);
  return match ? decodeURIComponent(match[1]) : url;
}

function normalizeCapabilities(raw: unknown): string[] {
  if (!raw) return [];
  if (Array.isArray(raw)) return raw.map(String);
  if (typeof raw === 'string')
    return raw.split(',').map((s) => s.trim()).filter(Boolean);
  return [];
}
