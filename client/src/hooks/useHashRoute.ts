import { useState, useEffect } from 'react';

/**
 * Lightweight hash-based router hook.
 * Returns the current hash path (without the leading '#').
 * Defaults to '/' when no hash is set.
 */
export function useHashRoute(): string {
  const [hash, setHash] = useState(() => window.location.hash.slice(1) || '/');

  useEffect(() => {
    const handler = () => setHash(window.location.hash.slice(1) || '/');
    window.addEventListener('hashchange', handler);
    return () => window.removeEventListener('hashchange', handler);
  }, []);

  return hash;
}
