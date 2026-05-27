/**
 * Agent Memory
 *
 * Pod-backed per-agent memory with WAC isolation.
 *
 * Layout:  /agents/{agentId}/memory/{key}.jsonld
 *
 * Containers are lazily created on first write. The caller is responsible for
 * passing a resolved pod path prefix so this module stays stateless.
 */

import { createLogger } from '../../utils/loggerConfig';
import { nostrAuth } from '../nostrAuthService';
import {
  JsonLdDocument,
  sanitizePreferenceKey,
  resolvePath,
  fetchWithAuth,
  fetchJsonLd,
  putResource,
  deleteResource,
  resourceExists,
} from './ldpClient';
import { writeContainerAcl } from './wacManager';

const logger = createLogger('SolidPodService:agentMemory');

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

function sanitizeId(id: string): string {
  let s = id.replace(/[\/\\\.]{2,}/g, '');
  s = s.replace(/[\/\\]/g, '-');
  s = s.replace(/^[.\-]+/, '');
  if (!s) throw new Error('Invalid agent ID');
  return s;
}

/** Derive the memory container path from the pod URL and agent ID. */
export function agentMemoryContainerPath(podPath: string, agentId: string): string {
  const safeId = sanitizeId(agentId);
  return `${podPath}/agents/${safeId}/memory/`;
}

/**
 * Ensure the agent's memory container (and parent agent container) exist,
 * creating them via PUT with an LDP BasicContainer Link header if absent.
 *
 * @returns The container path, or null on failure.
 */
export async function ensureAgentContainer(
  podPath: string,
  agentId: string
): Promise<string | null> {
  const safeId = sanitizeId(agentId);
  const agentContainerPath = `${podPath}/agents/${safeId}/`;
  const memoryContainerPath = `${podPath}/agents/${safeId}/memory/`;

  for (const path of [agentContainerPath, memoryContainerPath]) {
    const exists = await resourceExists(path);
    if (!exists) {
      const response = await fetchWithAuth(resolvePath(path), {
        method: 'PUT',
        headers: {
          'Content-Type': 'text/turtle',
          Link: '<http://www.w3.org/ns/ldp#BasicContainer>; rel="type"',
        },
        body: '',
      });
      if (!response.ok && response.status !== 409) {
        logger.error('Failed to create agent container', { path, status: response.status });
        return null;
      }
    }
  }

  return memoryContainerPath;
}

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

/** Store a memory entry in the pod. */
export async function storeAgentMemory(
  podPath: string,
  agentId: string,
  entry: { key: string; value: string; namespace: string; tags?: string[]; timestamp?: string }
): Promise<boolean> {
  const containerPath = await ensureAgentContainer(podPath, agentId);
  if (!containerPath) return false;

  const safeKey = sanitizePreferenceKey(entry.key);
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

  const success = await putResource(`${containerPath}${safeKey}.jsonld`, doc);
  if (success) {
    logger.info('Agent memory stored in Pod', {
      agentId,
      key: entry.key,
      namespace: entry.namespace,
    });
  }
  return success;
}

/** List all memory entries for an agent. */
export async function listAgentMemories(
  podPath: string,
  agentId: string
): Promise<Array<{ key: string; value: string; namespace: string }>> {
  const containerPath = agentMemoryContainerPath(podPath, agentId);

  try {
    const url = resolvePath(containerPath);
    const response = await fetchWithAuth(url, { headers: { Accept: 'application/ld+json' } });
    if (!response.ok) return [];

    const data = await response.json();
    const contains = data['ldp:contains'] || data['contains'] || [];
    const items: Array<{ '@id'?: string; url?: string }> = Array.isArray(contains)
      ? contains
      : [contains];

    const results: Array<{ key: string; value: string; namespace: string }> = [];

    for (const item of items) {
      const itemUrl = item['@id'] || item.url || '';
      const match = itemUrl.match(/\/([^/]+)\.jsonld$/);
      if (!match) continue;

      const key = decodeURIComponent(match[1]);
      try {
        const doc = await fetchJsonLd(
          itemUrl.startsWith('http') ? itemUrl : `${containerPath}${match[1]}.jsonld`
        );
        results.push({
          key: (doc as { identifier?: string }).identifier || key,
          value: (doc as { text?: string }).text || '',
          namespace:
            (doc as { additionalProperty?: { value?: string } }).additionalProperty?.value || '',
        });
      } catch {
        results.push({ key, value: '', namespace: '' });
      }
    }

    return results;
  } catch (error) {
    logger.error('Failed to list agent memories', { agentId, error });
    return [];
  }
}

/** Fetch a single memory entry by key. */
export async function getAgentMemory(
  podPath: string,
  agentId: string,
  key: string
): Promise<Record<string, unknown> | null> {
  const containerPath = agentMemoryContainerPath(podPath, agentId);
  try {
    const safeKey = sanitizePreferenceKey(key);
    return (await fetchJsonLd(`${containerPath}${safeKey}.jsonld`)) as Record<string, unknown>;
  } catch {
    logger.debug('Agent memory not found', { agentId, key });
    return null;
  }
}

/** Delete a memory entry. Returns true if deleted or already absent. */
export async function deleteAgentMemory(
  podPath: string,
  agentId: string,
  key: string
): Promise<boolean> {
  const containerPath = agentMemoryContainerPath(podPath, agentId);
  const safeKey = sanitizePreferenceKey(key);
  return deleteResource(`${containerPath}${safeKey}.jsonld`);
}

/** Set WAC permissions for an agent's memory container. */
export async function setAgentMemoryAccess(
  podPath: string,
  agentId: string,
  ownerWebId: string,
  permissions: { agentWebId: string; modes: ('Read' | 'Write' | 'Append')[] }
): Promise<boolean> {
  const containerPath = agentMemoryContainerPath(podPath, agentId);
  const ok = await writeContainerAcl(containerPath, ownerWebId, permissions);
  if (ok) {
    logger.info('Agent memory ACL updated', {
      agentId,
      agentWebId: permissions.agentWebId,
      modes: permissions.modes,
    });
  }
  return ok;
}
