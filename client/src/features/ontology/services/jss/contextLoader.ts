/**
 * contextLoader — fetch and cache the JSS ontology resource URL.
 *
 * Owns the base-URL configuration constants and the authenticated
 * HTTP helper so every other sub-module can import from one place.
 */

import { createLogger } from '../../../../utils/loggerConfig';
import { nostrAuth } from '../../../../services/nostrAuthService';

export const logger = createLogger('JssOntologyService');

export const JSS_BASE_URL = import.meta.env.VITE_JSS_URL || '/solid';
export const JSS_WS_URL = import.meta.env.VITE_JSS_WS_URL || null;
export const ONTOLOGY_RESOURCE_PATH =
  import.meta.env.VITE_JSS_ONTOLOGY_PATH || '/public/ontology';

export function getOntologyUrl(): string {
  return `${JSS_BASE_URL}${ONTOLOGY_RESOURCE_PATH}`;
}

export async function fetchWithAuth(
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
      try {
        const method = (options.method || 'GET').toUpperCase();
        const body = typeof options.body === 'string' ? options.body : undefined;
        const token = await nostrAuth.signRequest(url, method, body);
        headers.set('Authorization', `Nostr ${token}`);
      } catch (e) {
        logger.warn('NIP-98 signing failed:', e);
      }
    }
  }

  return fetch(url, {
    ...options,
    headers,
    credentials: 'include',
  });
}
