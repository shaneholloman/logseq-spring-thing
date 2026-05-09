import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';

vi.mock('../../../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

import { PollingPerformanceMonitor } from '../pollingPerformance';

describe('PollingPerformanceMonitor', () => {
  let monitor: PollingPerformanceMonitor;

  beforeEach(() => {
    monitor = new PollingPerformanceMonitor();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  // ---- Initial state ----

  describe('initial state', () => {
    it('should start with zero poll count', () => {
      const metrics = monitor.getMetrics();
      expect(metrics.pollCount).toBe(0);
      expect(metrics.successCount).toBe(0);
      expect(metrics.errorCount).toBe(0);
    });

    it('should have Infinity as initial minDuration', () => {
      const metrics = monitor.getMetrics();
      expect(metrics.minDuration).toBe(Infinity);
    });

    it('should report 100% success rate when no polls recorded', () => {
      expect(monitor.getSuccessRate()).toBe(1);
    });

    it('should report Infinity data freshness when no polls recorded', () => {
      expect(monitor.getDataFreshness()).toBe(Infinity);
    });
  });

  // ---- recordPoll ----

  describe('recordPoll', () => {
    it('should increment pollCount and successCount', () => {
      monitor.recordPoll(100, false);
      const metrics = monitor.getMetrics();
      expect(metrics.pollCount).toBe(1);
      expect(metrics.successCount).toBe(1);
    });

    it('should track data change count when data changed', () => {
      monitor.recordPoll(100, true);
      monitor.recordPoll(50, false);
      monitor.recordPoll(75, true);
      expect(monitor.getMetrics().dataChangeCount).toBe(2);
    });

    it('should compute min, max, and average duration', () => {
      monitor.recordPoll(100, false);
      monitor.recordPoll(200, false);
      monitor.recordPoll(300, false);

      const metrics = monitor.getMetrics();
      expect(metrics.minDuration).toBe(100);
      expect(metrics.maxDuration).toBe(300);
      expect(metrics.averageDuration).toBe(200);
    });

    it('should handle single poll duration correctly', () => {
      monitor.recordPoll(42, false);
      const metrics = monitor.getMetrics();
      expect(metrics.minDuration).toBe(42);
      expect(metrics.maxDuration).toBe(42);
      expect(metrics.averageDuration).toBe(42);
    });

    it('should update lastPollTime', () => {
      const now = Date.now();
      monitor.recordPoll(100, false);
      expect(monitor.getMetrics().lastPollTime).toBeGreaterThanOrEqual(now);
    });
  });

  // ---- recordError ----

  describe('recordError', () => {
    it('should increment error count and poll count', () => {
      monitor.recordError();
      const metrics = monitor.getMetrics();
      expect(metrics.errorCount).toBe(1);
      expect(metrics.pollCount).toBe(1);
      expect(metrics.successCount).toBe(0);
    });

    it('should affect success rate', () => {
      monitor.recordPoll(100, false); // success
      monitor.recordError(); // error
      expect(monitor.getSuccessRate()).toBe(0.5);
    });
  });

  // ---- getSuccessRate ----

  describe('getSuccessRate', () => {
    it('should return correct rate after mixed results', () => {
      monitor.recordPoll(100, false);
      monitor.recordPoll(100, false);
      monitor.recordPoll(100, false);
      monitor.recordError();

      expect(monitor.getSuccessRate()).toBeCloseTo(0.75, 2);
    });

    it('should return 0 when all polls are errors', () => {
      monitor.recordError();
      monitor.recordError();
      expect(monitor.getSuccessRate()).toBe(0);
    });
  });

  // ---- reset ----

  describe('reset', () => {
    it('should clear all metrics', () => {
      monitor.recordPoll(100, true);
      monitor.recordPoll(200, false);
      monitor.recordError();
      monitor.reset();

      const metrics = monitor.getMetrics();
      expect(metrics.pollCount).toBe(0);
      expect(metrics.successCount).toBe(0);
      expect(metrics.errorCount).toBe(0);
      expect(metrics.dataChangeCount).toBe(0);
      expect(metrics.averageDuration).toBe(0);
      expect(metrics.minDuration).toBe(Infinity);
      expect(metrics.maxDuration).toBe(0);
    });

    it('should reset success rate to 100%', () => {
      monitor.recordError();
      expect(monitor.getSuccessRate()).toBe(0);
      monitor.reset();
      expect(monitor.getSuccessRate()).toBe(1);
    });
  });

  // ---- getSummary ----

  describe('getSummary', () => {
    it('should return formatted summary string', () => {
      monitor.recordPoll(100, true);
      monitor.recordPoll(200, false);
      const summary = monitor.getSummary();

      expect(summary).toContain('Polls: 2');
      expect(summary).toContain('Success: 100.0%');
      expect(summary).toContain('Changes: 50.0%');
      expect(summary).toContain('Avg: 150ms');
    });

    it('should handle zero polls in summary', () => {
      const summary = monitor.getSummary();
      expect(summary).toContain('Polls: 0');
      expect(summary).toContain('Changes: 0.0%');
    });
  });

  // ---- getDataFreshness ----

  describe('getDataFreshness', () => {
    it('should return time since last poll', () => {
      monitor.recordPoll(100, false);
      vi.advanceTimersByTime(5000);
      const freshness = monitor.getDataFreshness();
      expect(freshness).toBeGreaterThanOrEqual(5000);
    });
  });

  // ---- Duration history cap ----

  describe('duration history', () => {
    it('should cap duration history at 100 entries', () => {
      for (let i = 0; i < 150; i++) {
        monitor.recordPoll(i, false);
      }
      // Average should be based on last 100 entries (50-149)
      const metrics = monitor.getMetrics();
      expect(metrics.averageDuration).toBeCloseTo(99.5, 0);
    });
  });
});
