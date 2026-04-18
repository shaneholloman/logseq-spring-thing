/**
 * Type-safe fetch wrapper with response status checking AND auth injection.
 *
 * Historically this wrapper called the bare `fetch()` — the rest of the codebase
 * goes through `UnifiedApiClient` which injects NIP-98 or dev-mode headers via
 * `authRequestInterceptor`. Enterprise panels (KPI, connectors, policies, etc.)
 * used to bypass that and hit 403 Forbidden. We now mirror the interceptor's
 * behaviour so any direct consumer of `apiFetch` gets the same auth guarantee.
 */
import { nostrAuth } from '../services/nostrAuthService';

export class ApiError extends Error {
  constructor(
    public status: number,
    public statusText: string,
    message?: string,
  ) {
    super(message || `API error ${status}: ${statusText}`);
    this.name = 'ApiError';
  }
}

async function injectAuthHeaders(url: string, init?: RequestInit): Promise<RequestInit> {
  const headers = new Headers(init?.headers || {});

  if (nostrAuth.isAuthenticated()) {
    const user = nostrAuth.getCurrentUser();

    if (nostrAuth.isDevMode()) {
      if (!headers.has('Authorization')) {
        headers.set('Authorization', 'Bearer dev-session-token');
      }
      if (user?.pubkey && !headers.has('X-Nostr-Pubkey')) {
        headers.set('X-Nostr-Pubkey', user.pubkey);
      }
    } else if (user?.pubkey && !headers.has('Authorization')) {
      try {
        const fullUrl = new URL(url, window.location.origin).href;
        const method = (init?.method || 'GET').toUpperCase();
        const body = typeof init?.body === 'string' ? init.body : undefined;
        const token = await nostrAuth.signRequest(fullUrl, method, body);
        headers.set('Authorization', `Nostr ${token}`);
      } catch {
        // NIP-98 signing failed — let the request fly unsigned and surface 401.
      }
    }
  }

  return { ...init, headers };
}

export async function apiFetch<T>(url: string, init?: RequestInit): Promise<T> {
  const withAuth = await injectAuthHeaders(url, init);
  const response = await fetch(url, withAuth);
  if (!response.ok) {
    let detail = response.statusText;
    try {
      const body = await response.json();
      if (body.error) detail = body.error;
    } catch {
      // body not JSON, use statusText
    }
    throw new ApiError(response.status, response.statusText, detail);
  }
  return response.json();
}

export async function apiPost<T>(url: string, body: unknown): Promise<T> {
  return apiFetch<T>(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}
