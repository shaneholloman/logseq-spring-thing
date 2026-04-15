/**
 * Type-safe fetch wrapper with response status checking.
 * Fixes QE finding: 6 components call fetch() without checking response.ok.
 */
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

export async function apiFetch<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, init);
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
