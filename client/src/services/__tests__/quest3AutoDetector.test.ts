/**
 * Quest3AutoDetector Tests
 *
 * RED phase: All tests target Quest3AutoDetector singleton behavior --
 * user-agent detection, URL param override, XR session init features,
 * Quest 3 settings configuration, and network adapter graceful degradation.
 *
 * Mocks: navigator.userAgent, navigator.xr, window.location, platformManager store,
 *        settingsStore, XRNetworkAdapter.
 */

import { describe, it, expect, beforeEach, afterEach, vi, type Mock } from 'vitest';
import type { XRNetworkAdapter } from '../../xr/adapters/XRNetworkAdapter';

// -- Module mocks -------------------------------------------------------------

// Mock loggerConfig before any imports that use it
vi.mock('../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

// Mock settingsStore
const mockUpdateSettings = vi.fn();
vi.mock('../../store/settingsStore', () => ({
  useSettingsStore: {
    getState: () => ({
      updateSettings: mockUpdateSettings,
    }),
  },
}));

// Mock NullAdapter so the default import resolves
vi.mock('../../xr/adapters/NullAdapter', () => ({
  NullAdapter: class NullAdapter {
    connect = vi.fn().mockResolvedValue(undefined);
    disconnect = vi.fn().mockResolvedValue(undefined);
    isConnected = vi.fn().mockReturnValue(false);
    onStateChange = vi.fn();
  },
}));

// Mock platformManager store -- keep a mutable ref so tests can override
const platformStoreState: Record<string, any> = {
  platform: 'unknown',
  isXRMode: false,
  forceVRMode: false,
  setXRMode: vi.fn(),
  setXRSessionState: vi.fn(),
};

vi.mock('../platformManager', () => ({
  usePlatformStore: {
    getState: () => platformStoreState,
  },
}));

// -- Helpers ------------------------------------------------------------------

/**
 * Sets `navigator.userAgent` to the given string for the duration of a test.
 */
function setUserAgent(ua: string): void {
  Object.defineProperty(navigator, 'userAgent', {
    value: ua,
    writable: true,
    configurable: true,
  });
}

/**
 * Sets `window.location.search` to the given query string.
 */
function setLocationSearch(search: string): void {
  Object.defineProperty(window, 'location', {
    value: { ...window.location, search },
    writable: true,
    configurable: true,
  });
}

/**
 * Installs a mock `navigator.xr` with configurable session support results.
 */
function setNavigatorXR(options: {
  vrSupported?: boolean;
  arSupported?: boolean;
  requestSessionResult?: any;
} = {}): void {
  const { vrSupported = false, arSupported = false, requestSessionResult } = options;

  const xr = {
    isSessionSupported: vi.fn().mockImplementation((mode: string) => {
      if (mode === 'immersive-vr') return Promise.resolve(vrSupported);
      if (mode === 'immersive-ar') return Promise.resolve(arSupported);
      return Promise.resolve(false);
    }),
    requestSession: vi.fn().mockResolvedValue(
      requestSessionResult ?? {
        environmentBlendMode: 'alpha-blend',
        supportedFrameRates: [72, 90, 120],
        inputSources: [],
      },
    ),
  };

  Object.defineProperty(navigator, 'xr', {
    value: xr,
    writable: true,
    configurable: true,
  });
}

function clearNavigatorXR(): void {
  Object.defineProperty(navigator, 'xr', {
    value: undefined,
    writable: true,
    configurable: true,
  });
}

/**
 * Creates a mock XRNetworkAdapter with spy methods.
 */
function createMockAdapter(overrides: Partial<XRNetworkAdapter> = {}): XRNetworkAdapter & {
  connect: Mock;
  disconnect: Mock;
  isConnected: Mock;
  onStateChange: Mock;
} {
  return {
    connect: vi.fn().mockResolvedValue(undefined),
    disconnect: vi.fn().mockResolvedValue(undefined),
    isConnected: vi.fn().mockReturnValue(false),
    onStateChange: vi.fn(),
    ...overrides,
  };
}

// -- Import under test (after mocks) -----------------------------------------

import { Quest3AutoDetector, type Quest3DetectionResult } from '../quest3AutoDetector';

// -- Test suites --------------------------------------------------------------

describe('Quest3AutoDetector', () => {
  let detector: Quest3AutoDetector;

  beforeEach(() => {
    // Reset singleton internal state between tests
    (Quest3AutoDetector as any).instance = undefined;
    detector = Quest3AutoDetector.getInstance();

    // Defaults
    setUserAgent('');
    setLocationSearch('');
    clearNavigatorXR();
    platformStoreState.platform = 'unknown';
    platformStoreState.isXRMode = false;
    platformStoreState.forceVRMode = false;

    vi.clearAllMocks();
  });

  afterEach(async () => {
    await detector.resetDetection();
  });

  // -- Singleton pattern ----------------------------------------------------

  describe('Singleton pattern', () => {
    it('should return the same instance on repeated calls', () => {
      const a = Quest3AutoDetector.getInstance();
      const b = Quest3AutoDetector.getInstance();
      expect(a).toBe(b);
    });

    it('should not expose a public constructor', () => {
      const instance = Quest3AutoDetector.getInstance();
      expect(instance).toBeInstanceOf(Quest3AutoDetector);
    });
  });

  // -- User-agent detection (positive cases) --------------------------------

  describe('Quest 3 user-agent detection - positive cases', () => {
    it('should detect "Quest 3" in user agent as Quest 3 hardware', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) AppleWebKit/537.36 OculusBrowser/33.0');
      setNavigatorXR();

      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3).toBe(true);
      expect(result.isQuest3Browser).toBe(true);
    });

    it('should detect "Quest3" (no space) in user agent as Quest 3 hardware', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest3) AppleWebKit/537.36');
      setNavigatorXR();

      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3).toBe(true);
    });

    it('should detect "OculusBrowser" as Quest 3 browser', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12) AppleWebKit/537.36 OculusBrowser/33.0');
      setNavigatorXR();

      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3Browser).toBe(true);
      expect(result.isQuest3).toBe(false);
    });

    it('should detect "Mobile VR" combination as Quest 3 browser', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Mobile; VR) AppleWebKit/537.36');
      setNavigatorXR();

      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3Browser).toBe(true);
    });

    it('should detect "Quest" without version as Quest 3 browser', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest) AppleWebKit/537.36 OculusBrowser/28.0');
      setNavigatorXR();

      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3Browser).toBe(true);
      expect(result.isQuest3).toBe(false);
    });
  });

  // -- User-agent detection (negative cases) --------------------------------

  describe('Quest 3 user-agent detection - negative cases', () => {
    it('should not detect desktop Chrome as Quest 3', async () => {
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0.0.0');
      setNavigatorXR();

      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3).toBe(false);
      expect(result.isQuest3Browser).toBe(false);
    });

    it('should not detect mobile Safari as Quest 3', async () => {
      setUserAgent('Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148 Safari/604.1');
      setNavigatorXR();

      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3).toBe(false);
      expect(result.isQuest3Browser).toBe(false);
    });

    it('should not detect Pico headset as Quest 3', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Pico Neo 3 Link) AppleWebKit/537.36 PicoBrowser/2.0');
      setNavigatorXR();

      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3).toBe(false);
      expect(result.isQuest3Browser).toBe(false);
    });

    it('should not detect empty user agent as Quest 3', async () => {
      setUserAgent('');
      setNavigatorXR();

      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3).toBe(false);
      expect(result.isQuest3Browser).toBe(false);
    });
  });

  // -- URL parameter override -----------------------------------------------

  describe('URL parameter override', () => {
    it('should force Quest 3 mode when forceVRMode is true', async () => {
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
      platformStoreState.forceVRMode = true;

      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3).toBe(true);
      expect(result.isQuest3Browser).toBe(true);
      expect(result.shouldAutoStart).toBe(true);
    });

    it('should not force Quest 3 mode when forceVRMode is false', async () => {
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
      platformStoreState.forceVRMode = false;
      setNavigatorXR();

      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3).toBe(false);
    });

    it('should not force Quest 3 mode when forceVRMode is absent', async () => {
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
      setLocationSearch('?foo=bar');
      setNavigatorXR();

      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3).toBe(false);
    });
  });

  // -- AR support detection -------------------------------------------------

  describe('AR support detection', () => {
    it('should report supportsAR=true when navigator.xr confirms immersive-ar', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });

      const result = await detector.detectQuest3Environment();

      expect(result.supportsAR).toBe(true);
    });

    it('should report supportsAR=false when navigator.xr denies immersive-ar', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: false });

      const result = await detector.detectQuest3Environment();

      expect(result.supportsAR).toBe(false);
    });

    it('should report supportsAR=false when navigator.xr is absent', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      clearNavigatorXR();

      const result = await detector.detectQuest3Environment();

      expect(result.supportsAR).toBe(false);
    });

    it('should handle navigator.xr.isSessionSupported throwing an error gracefully', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      Object.defineProperty(navigator, 'xr', {
        value: {
          isSessionSupported: vi.fn().mockRejectedValue(new Error('XR unavailable')),
        },
        writable: true,
        configurable: true,
      });

      const result = await detector.detectQuest3Environment();

      expect(result.supportsAR).toBe(false);
    });
  });

  // -- Auto-start conditions ------------------------------------------------

  describe('Auto-start conditions', () => {
    it('should set shouldAutoStart=true when Quest 3 hardware + browser + AR supported', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });

      const result = await detector.detectQuest3Environment();

      expect(result.shouldAutoStart).toBe(true);
    });

    it('should set shouldAutoStart=false when Quest 3 hardware but no AR support', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: false });

      const result = await detector.detectQuest3Environment();

      expect(result.shouldAutoStart).toBe(false);
    });

    it('should set shouldAutoStart=false when browser detected but not Quest 3 hardware', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 2) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });

      const result = await detector.detectQuest3Environment();

      expect(result.shouldAutoStart).toBe(false);
    });
  });

  // -- XR session init features ---------------------------------------------

  describe('XR session init features', () => {
    it('should request immersive-ar session with correct required and optional features', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });

      await detector.autoStartQuest3AR();

      const xr = navigator.xr as any;
      expect(xr.requestSession).toHaveBeenCalledWith('immersive-ar', {
        requiredFeatures: ['local-floor'],
        optionalFeatures: [
          'hand-tracking',
          'hit-test',
          'anchors',
          'plane-detection',
          'light-estimation',
          'depth-sensing',
          'mesh-detection',
        ],
      });
    });
  });

  // -- Settings configuration -----------------------------------------------

  describe('Settings configuration', () => {
    it('should configure passthrough, hand tracking, and quality=high on auto-start', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });

      await detector.autoStartQuest3AR();

      expect(mockUpdateSettings).toHaveBeenCalledTimes(1);
      const updater = mockUpdateSettings.mock.calls[0][0];
      expect(updater).toBeTypeOf('function');

      const draft = {
        xr: {},
        auth: {},
        visualisation: { rendering: {}, physics: {} },
        system: { debug: {} },
      };
      updater(draft);
      expect(draft.xr).toMatchObject({
        enabled: true,
        enableHandTracking: true,
        enablePassthroughPortal: true,
        quality: 'high',
        passthroughOpacity: 1.0,
      });
    });
  });

  // -- Detection result caching ---------------------------------------------

  describe('Detection result caching', () => {
    it('should return cached result on second call without re-detecting', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });
      const firstResult = await detector.detectQuest3Environment();

      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
      const secondResult = await detector.detectQuest3Environment();

      expect(secondResult).toBe(firstResult);
      expect(secondResult.isQuest3).toBe(true);
    });
  });

  // -- resetDetection -------------------------------------------------------

  describe('resetDetection', () => {
    it('should clear cached detection results and allow fresh detection', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });
      await detector.detectQuest3Environment();

      await detector.resetDetection();
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
      setNavigatorXR({ arSupported: false });
      const result = await detector.detectQuest3Environment();

      expect(result.isQuest3).toBe(false);
      expect(result.isQuest3Browser).toBe(false);
    });

    it('should disconnect network adapter on reset', async () => {
      const mockAdapter = createMockAdapter();
      (Quest3AutoDetector as any).instance = undefined;
      detector = Quest3AutoDetector.getInstance(mockAdapter);

      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });
      await detector.autoStartQuest3AR();

      await detector.resetDetection();

      expect(mockAdapter.disconnect).toHaveBeenCalled();
    });

    it('should allow autoStartQuest3AR to be retried after reset', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });
      await detector.autoStartQuest3AR();

      await detector.resetDetection();
      setNavigatorXR({ arSupported: true });
      const result = await detector.autoStartQuest3AR();

      expect(result).toBe(true);
    });
  });

  // -- autoStartQuest3AR idempotency ----------------------------------------

  describe('autoStartQuest3AR idempotency', () => {
    it('should return false on second call without reset', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });
      await detector.autoStartQuest3AR();

      const secondResult = await detector.autoStartQuest3AR();

      expect(secondResult).toBe(false);
    });
  });

  // -- Network adapter connection failure - graceful degradation ------------

  describe('Network adapter connection failure', () => {
    it('should not crash or throw when network adapter connect fails', async () => {
      const mockAdapter = createMockAdapter({
        connect: vi.fn().mockRejectedValue(new Error('Connection refused')),
      });
      (Quest3AutoDetector as any).instance = undefined;
      detector = Quest3AutoDetector.getInstance(mockAdapter);

      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });

      // connect() rejection propagates through autoStartQuest3AR's try/catch
      // and sets autoStartAttempted = false, returning false. With the adapter
      // pattern, connect() throwing bubbles to the outer catch, which is the
      // correct adapter contract.
      const result = await detector.autoStartQuest3AR();

      // The adapter threw, so the outer catch fires and returns false.
      expect(result).toBe(false);
    });

    it('should return NullAdapter when no adapter injected', () => {
      const adapter = detector.getNetworkAdapter();
      expect(adapter).toBeDefined();
      expect(adapter.isConnected()).toBe(false);
    });
  });

  // -- Network adapter integration -----------------------------------------

  describe('Network adapter integration', () => {
    it('should call adapter.connect() during autoStartQuest3AR', async () => {
      const mockAdapter = createMockAdapter();
      (Quest3AutoDetector as any).instance = undefined;
      detector = Quest3AutoDetector.getInstance(mockAdapter);

      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });

      await detector.autoStartQuest3AR();

      expect(mockAdapter.connect).toHaveBeenCalledTimes(1);
    });

    it('should call adapter.disconnect() during resetDetection', async () => {
      const mockAdapter = createMockAdapter();
      (Quest3AutoDetector as any).instance = undefined;
      detector = Quest3AutoDetector.getInstance(mockAdapter);

      await detector.resetDetection();

      expect(mockAdapter.disconnect).toHaveBeenCalledTimes(1);
    });
  });

  // -- isInQuest3ARMode -----------------------------------------------------

  describe('isInQuest3ARMode', () => {
    it('should return true when platform is quest3, XR mode on, and shouldAutoStart', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });
      platformStoreState.platform = 'quest3';
      platformStoreState.isXRMode = true;
      await detector.autoStartQuest3AR();

      const inARMode = detector.isInQuest3ARMode();

      expect(inARMode).toBe(true);
    });

    it('should return false when XR mode is off', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });
      platformStoreState.platform = 'quest3';
      platformStoreState.isXRMode = false;
      await detector.detectQuest3Environment();

      const inARMode = detector.isInQuest3ARMode();

      expect(inARMode).toBe(false);
    });
  });

  // -- Detection result shape -----------------------------------------------

  describe('Detection result shape', () => {
    it('should return an object conforming to Quest3DetectionResult interface', async () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ arSupported: true });

      const result = await detector.detectQuest3Environment();

      expect(result).toEqual(
        expect.objectContaining({
          isQuest3: expect.any(Boolean),
          isQuest3Browser: expect.any(Boolean),
          supportsAR: expect.any(Boolean),
          shouldAutoStart: expect.any(Boolean),
        }),
      );
    });
  });
});
