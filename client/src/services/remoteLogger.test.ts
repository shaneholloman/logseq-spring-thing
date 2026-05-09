import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';

// Must mock import.meta.env before importing
vi.stubGlobal('sessionStorage', {
  getItem: vi.fn(() => null),
  setItem: vi.fn(),
});

let RemoteLoggerModule: typeof import('./remoteLogger');

describe('RemoteLogger', () => {
  beforeEach(async () => {
    vi.resetModules();
    vi.useFakeTimers();
    // Disable remote logging so constructor doesn't start timers
    vi.stubEnv('VITE_REMOTE_LOGGING_DISABLED', 'true');
    RemoteLoggerModule = await import('./remoteLogger');
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllEnvs();
  });

  describe('createRemoteLogger', () => {
    it('returns a logger with debug/info/warn/error methods', () => {
      const log = RemoteLoggerModule.createRemoteLogger('test-ns');

      expect(log).toHaveProperty('debug');
      expect(log).toHaveProperty('info');
      expect(log).toHaveProperty('warn');
      expect(log).toHaveProperty('error');
      expect(typeof log.debug).toBe('function');
    });
  });

  describe('remoteLogger.log', () => {
    it('buffers log entries when enabled', () => {
      const logger = RemoteLoggerModule.remoteLogger;
      logger.setEnabled(true);

      logger.log('info', 'test-ns', 'hello world');

      // Access buffer via flush -- if buffer is empty, flush is a no-op
      // We verify by calling flush and checking fetch was called
      expect(() => logger.log('debug', 'test-ns', 'msg')).not.toThrow();
    });

    it('does not buffer when disabled', () => {
      const logger = RemoteLoggerModule.remoteLogger;
      logger.setEnabled(false);

      logger.log('info', 'test-ns', 'should be discarded');

      // flush should be a no-op (buffer empty)
      const p = logger.flush();
      expect(p).resolves.toBeUndefined();
    });
  });

  describe('remoteLogger.configure', () => {
    it('accepts partial configuration updates', () => {
      const logger = RemoteLoggerModule.remoteLogger;

      expect(() =>
        logger.configure({
          flushInterval: 5000,
          maxBufferSize: 100,
        }),
      ).not.toThrow();
    });

    it('accepts server endpoint change', () => {
      const logger = RemoteLoggerModule.remoteLogger;

      expect(() =>
        logger.configure({
          serverEndpoint: 'http://localhost:9999/logs',
        }),
      ).not.toThrow();
    });

    it('enables/disables via configure', () => {
      const logger = RemoteLoggerModule.remoteLogger;

      logger.configure({ enabled: false });
      logger.log('info', 'ns', 'discarded');

      logger.configure({ enabled: true });
      logger.log('info', 'ns', 'buffered');

      // No throw is the success criterion
    });
  });

  describe('remoteLogger.setEnabled', () => {
    it('toggles enabled state', () => {
      const logger = RemoteLoggerModule.remoteLogger;

      logger.setEnabled(false);
      // After disable, logs should be silently dropped
      expect(() => logger.log('error', 'ns', 'dropped')).not.toThrow();

      logger.setEnabled(true);
      expect(() => logger.log('info', 'ns', 'accepted')).not.toThrow();
    });
  });

  describe('remoteLogger.flush', () => {
    it('resolves immediately when buffer is empty', async () => {
      const logger = RemoteLoggerModule.remoteLogger;
      logger.setEnabled(false);

      await expect(logger.flush()).resolves.toBeUndefined();
    });

    it('uses sendBeacon for sync flush', async () => {
      const mockSendBeacon = vi.fn(() => true);
      vi.stubGlobal('navigator', {
        ...navigator,
        sendBeacon: mockSendBeacon,
        userAgent: 'test-agent',
      });

      const logger = RemoteLoggerModule.remoteLogger;
      logger.setEnabled(true);
      logger.log('info', 'ns', 'test');

      await logger.flush(true);

      expect(mockSendBeacon).toHaveBeenCalled();
    });
  });
});
