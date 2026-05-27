/**
 * Worker-safe logger.
 * createLogger depends on localStorage/window which are unavailable in Workers.
 * Only warn/error by default; set self.__WORKER_DEBUG = true in devtools to enable info/debug.
 */
const workerSelf = self as unknown as Record<string, unknown>;

export const workerLogger = {
  info: (...args: unknown[]) => { if (workerSelf.__WORKER_DEBUG) console.log('[GraphWorker]', ...args); },
  warn: (...args: unknown[]) => console.warn('[GraphWorker]', ...args),
  error: (...args: unknown[]) => console.error('[GraphWorker]', ...args),
  debug: (...args: unknown[]) => { if (workerSelf.__WORKER_DEBUG) console.debug('[GraphWorker]', ...args); },
};
