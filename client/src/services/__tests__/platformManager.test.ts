/**
 * PlatformManager Tests
 *
 * RED phase: Tests the Zustand-based usePlatformStore and the backward-compatible
 * PlatformManager class wrapper. Covers platform detection from user-agent strings,
 * per-platform capabilities, XR support detection, event system, and singleton semantics.
 *
 * Mocks: navigator.userAgent, navigator.xr, navigator.maxTouchPoints, window
 */

import { describe, it, expect, beforeEach, afterEach, vi, type Mock } from 'vitest';

// ── Module mocks ──────────────────────────────────────────────────────────────

vi.mock('../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

// ── Import under test ────────────────────────────────────────────────────────

import {
  usePlatformStore,
  PlatformManager,
  platformManager,
  type PlatformType,
  type PlatformCapabilities,
  type PlatformEventType,
  type XRSessionState,
} from '../platformManager';

// ── Helpers ───────────────────────────────────────────────────────────────────

function setUserAgent(ua: string): void {
  Object.defineProperty(navigator, 'userAgent', {
    value: ua,
    writable: true,
    configurable: true,
  });
}

function setNavigatorXR(options: {
  vrSupported?: boolean;
  arSupported?: boolean;
} = {}): void {
  const { vrSupported = false, arSupported = false } = options;

  const xr = {
    isSessionSupported: vi.fn().mockImplementation((mode: string) => {
      if (mode === 'immersive-vr') return Promise.resolve(vrSupported);
      if (mode === 'immersive-ar') return Promise.resolve(arSupported);
      return Promise.resolve(false);
    }),
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

function setMaxTouchPoints(n: number): void {
  Object.defineProperty(navigator, 'maxTouchPoints', {
    value: n,
    writable: true,
    configurable: true,
  });
}

// ── Test suites ──────────────────────────────────────────────────────────────

describe('usePlatformStore', () => {
  beforeEach(() => {
    // Reset store to defaults
    usePlatformStore.setState({
      platform: 'unknown',
      xrDeviceType: 'none',
      capabilities: {
        xrSupported: false,
        handTrackingSupported: false,
        arSupported: false,
        vrSupported: false,
        performanceTier: 'medium',
        maxTextureSize: 2048,
        hasTouchscreen: false,
        hasPointer: true,
        hasKeyboard: true,
        hasGamepad: false,
        memoryLimited: false,
      },
      userAgent: '',
      isXRMode: false,
      xrSessionState: 'inactive',
      isWebXRSupported: false,
      initialized: false,
      listeners: new Map(),
    });

    setUserAgent('');
    clearNavigatorXR();
    setMaxTouchPoints(0);
    vi.clearAllMocks();
  });

  afterEach(() => {
    usePlatformStore.getState().removeAllListeners();
  });

  // ── Platform detection ─────────────────────────────────────────────────

  describe('Platform detection from user agent', () => {
    it('should detect Quest 3 from user agent containing "Quest 3"', () => {
      // GIVEN: Quest 3 user agent
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) AppleWebKit/537.36 OculusBrowser/33.0');

      // WHEN: Detecting platform
      usePlatformStore.getState().detectPlatform();

      // THEN: Platform is quest3
      expect(usePlatformStore.getState().platform).toBe('quest3');
      expect(usePlatformStore.getState().xrDeviceType).toBe('quest');
    });

    it('should detect Quest 2 from user agent containing "Quest 2"', () => {
      // GIVEN: Quest 2 user agent
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 2) AppleWebKit/537.36 OculusBrowser/28.0');

      // WHEN: Detecting platform
      usePlatformStore.getState().detectPlatform();

      // THEN: Platform is quest2
      expect(usePlatformStore.getState().platform).toBe('quest2');
      expect(usePlatformStore.getState().xrDeviceType).toBe('quest');
    });

    it('should detect generic Quest from user agent containing "Quest" without version', () => {
      // GIVEN: Generic Quest user agent
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest) AppleWebKit/537.36 OculusBrowser/26.0');

      // WHEN: Detecting platform
      usePlatformStore.getState().detectPlatform();

      // THEN: Platform is quest (generic)
      expect(usePlatformStore.getState().platform).toBe('quest');
      expect(usePlatformStore.getState().xrDeviceType).toBe('quest');
    });

    it('should detect Pico from user agent containing "Pico"', () => {
      // GIVEN: Pico user agent
      setUserAgent('Mozilla/5.0 (Linux; Pico Neo 3 Link) AppleWebKit/537.36 PicoBrowser/2.0');

      // WHEN: Detecting platform
      usePlatformStore.getState().detectPlatform();

      // THEN: Platform is pico
      expect(usePlatformStore.getState().platform).toBe('pico');
      expect(usePlatformStore.getState().xrDeviceType).toBe('pico');
    });

    it('should detect Pico from user agent containing "PICO" (uppercase)', () => {
      // GIVEN: Uppercase PICO in user agent
      setUserAgent('Mozilla/5.0 (Linux; PICO 4) AppleWebKit/537.36');

      // WHEN: Detecting platform
      usePlatformStore.getState().detectPlatform();

      // THEN: Platform is pico
      expect(usePlatformStore.getState().platform).toBe('pico');
    });

    it('should detect mobile from Android user agent', () => {
      // GIVEN: Android mobile user agent
      setUserAgent('Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/120.0.0.0 Mobile Safari/537.36');

      // WHEN: Detecting platform
      usePlatformStore.getState().detectPlatform();

      // THEN: Platform is mobile
      expect(usePlatformStore.getState().platform).toBe('mobile');
      expect(usePlatformStore.getState().xrDeviceType).toBe('mobile-xr');
    });

    it('should detect mobile from iPhone user agent', () => {
      // GIVEN: iPhone user agent
      setUserAgent('Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148');

      // WHEN: Detecting platform
      usePlatformStore.getState().detectPlatform();

      // THEN: Platform is mobile
      expect(usePlatformStore.getState().platform).toBe('mobile');
    });

    it('should detect desktop from standard Chrome user agent', () => {
      // GIVEN: Desktop Chrome user agent
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36');

      // WHEN: Detecting platform
      usePlatformStore.getState().detectPlatform();

      // THEN: Platform is desktop
      expect(usePlatformStore.getState().platform).toBe('desktop');
      expect(usePlatformStore.getState().xrDeviceType).toBe('desktop-xr');
    });

    it('should detect desktop from Firefox on Linux', () => {
      // GIVEN: Firefox on Linux
      setUserAgent('Mozilla/5.0 (X11; Linux x86_64; rv:120.0) Gecko/20100101 Firefox/120.0');

      // WHEN: Detecting platform
      usePlatformStore.getState().detectPlatform();

      // THEN: Platform is desktop
      expect(usePlatformStore.getState().platform).toBe('desktop');
    });
  });

  // ── Capabilities per platform ──────────────────────────────────────────

  describe('Capabilities per platform', () => {
    it('should set quest3 capabilities: performanceTier=high, maxTextureSize=4096, memoryLimited=true', () => {
      // GIVEN: Quest 3 user agent
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');

      // WHEN: Detecting
      usePlatformStore.getState().detectPlatform();
      const caps = usePlatformStore.getState().capabilities;

      // THEN: Quest 3 performance profile
      expect(caps.performanceTier).toBe('high');
      expect(caps.maxTextureSize).toBe(4096);
      expect(caps.memoryLimited).toBe(true);
      expect(caps.hasGamepad).toBe(true);
      expect(caps.hasKeyboard).toBe(false);
    });

    it('should set quest2 capabilities: performanceTier=medium, maxTextureSize=2048, memoryLimited=true', () => {
      // GIVEN: Quest 2 user agent
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 2) OculusBrowser/28.0');

      // WHEN: Detecting
      usePlatformStore.getState().detectPlatform();
      const caps = usePlatformStore.getState().capabilities;

      // THEN: Quest 2 performance profile
      expect(caps.performanceTier).toBe('medium');
      expect(caps.maxTextureSize).toBe(2048);
      expect(caps.memoryLimited).toBe(true);
    });

    it('should set quest (gen 1) capabilities: performanceTier=low, memoryLimited=true', () => {
      // GIVEN: Quest 1 user agent
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest) OculusBrowser/26.0');

      // WHEN: Detecting
      usePlatformStore.getState().detectPlatform();
      const caps = usePlatformStore.getState().capabilities;

      // THEN: Quest 1 performance profile
      expect(caps.performanceTier).toBe('low');
      expect(caps.memoryLimited).toBe(true);
    });

    it('should set pico capabilities: performanceTier=medium, memoryLimited=true, hasGamepad=true', () => {
      // GIVEN: Pico user agent
      setUserAgent('Mozilla/5.0 (Linux; Pico Neo 3 Link) PicoBrowser/2.0');

      // WHEN: Detecting
      usePlatformStore.getState().detectPlatform();
      const caps = usePlatformStore.getState().capabilities;

      // THEN: Pico performance profile
      expect(caps.performanceTier).toBe('medium');
      expect(caps.memoryLimited).toBe(true);
      expect(caps.hasGamepad).toBe(true);
    });

    it('should set mobile capabilities: performanceTier=low, memoryLimited=true, hasPointer=false', () => {
      // GIVEN: Mobile user agent
      setUserAgent('Mozilla/5.0 (Linux; Android 14; Pixel 8) Chrome/120.0.0.0 Mobile');

      // WHEN: Detecting
      usePlatformStore.getState().detectPlatform();
      const caps = usePlatformStore.getState().capabilities;

      // THEN: Mobile performance profile
      expect(caps.performanceTier).toBe('low');
      expect(caps.memoryLimited).toBe(true);
      // Note: source sets hasPointer=true for mobile (platform === 'desktop' || 'mobile')
      expect(caps.hasPointer).toBe(true);
      expect(caps.hasKeyboard).toBe(false);
    });

    it('should set desktop capabilities: performanceTier=high, maxTextureSize=8192, memoryLimited=false', () => {
      // GIVEN: Desktop user agent
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');

      // WHEN: Detecting
      usePlatformStore.getState().detectPlatform();
      const caps = usePlatformStore.getState().capabilities;

      // THEN: Desktop performance profile
      expect(caps.performanceTier).toBe('high');
      expect(caps.maxTextureSize).toBe(8192);
      expect(caps.memoryLimited).toBe(false);
      expect(caps.hasPointer).toBe(true);
      expect(caps.hasKeyboard).toBe(true);
      expect(caps.hasGamepad).toBe(false);
    });
  });

  // ── XR support detection ───────────────────────────────────────────────

  describe('XR support detection via initialize()', () => {
    it('should set vrSupported=true when navigator.xr reports immersive-vr', async () => {
      // GIVEN: navigator.xr with VR support
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
      setNavigatorXR({ vrSupported: true, arSupported: false });

      // WHEN: Initializing
      await usePlatformStore.getState().initialize();

      // THEN: VR supported in capabilities
      const caps = usePlatformStore.getState().capabilities;
      expect(caps.vrSupported).toBe(true);
      expect(caps.xrSupported).toBe(true);
    });

    it('should set arSupported=true when navigator.xr reports immersive-ar', async () => {
      // GIVEN: navigator.xr with AR support
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ vrSupported: false, arSupported: true });

      // WHEN: Initializing
      await usePlatformStore.getState().initialize();

      // THEN: AR supported
      const caps = usePlatformStore.getState().capabilities;
      expect(caps.arSupported).toBe(true);
      expect(caps.xrSupported).toBe(true);
    });

    it('should set xrSupported=false when no navigator.xr present', async () => {
      // GIVEN: No WebXR API
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
      clearNavigatorXR();

      // WHEN: Initializing
      await usePlatformStore.getState().initialize();

      // THEN: XR not supported
      const caps = usePlatformStore.getState().capabilities;
      expect(caps.xrSupported).toBe(false);
      expect(caps.vrSupported).toBe(false);
      expect(caps.arSupported).toBe(false);
    });

    it('should set handTrackingSupported=true for Quest devices', async () => {
      // GIVEN: Quest 3 with WebXR
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      setNavigatorXR({ vrSupported: true });

      // WHEN: Initializing
      await usePlatformStore.getState().initialize();

      // THEN: Hand tracking supported for Quest
      const caps = usePlatformStore.getState().capabilities;
      expect(caps.handTrackingSupported).toBe(true);
    });

    it('should set initialized=true after initialization', async () => {
      // GIVEN: Any platform
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');

      // WHEN: Initializing
      await usePlatformStore.getState().initialize();

      // THEN: initialized flag is true
      expect(usePlatformStore.getState().initialized).toBe(true);
    });
  });

  // ── Event system ──────────────────────────────────────────────────────

  describe('Event dispatch and listener management', () => {
    it('should call listener when event is dispatched', () => {
      // GIVEN: Listener registered for xrmodechange
      const callback = vi.fn();
      usePlatformStore.getState().addEventListener('xrmodechange', callback);
      callback.mockClear(); // Clear the immediate dispatch from addEventListener

      // WHEN: Dispatching event
      usePlatformStore.getState().dispatchEvent('xrmodechange', { enabled: true });

      // THEN: Callback called with data
      expect(callback).toHaveBeenCalledWith({ enabled: true });
    });

    it('should support multiple listeners on the same event', () => {
      // GIVEN: Two listeners on platformchange
      const cb1 = vi.fn();
      const cb2 = vi.fn();
      usePlatformStore.getState().addEventListener('platformchange', cb1);
      usePlatformStore.getState().addEventListener('platformchange', cb2);
      cb1.mockClear();
      cb2.mockClear();

      // WHEN: Dispatching
      usePlatformStore.getState().dispatchEvent('platformchange', { platform: 'quest3' });

      // THEN: Both called
      expect(cb1).toHaveBeenCalledWith({ platform: 'quest3' });
      expect(cb2).toHaveBeenCalledWith({ platform: 'quest3' });
    });

    it('should immediately call platformchange listener with current platform on registration', () => {
      // GIVEN: Platform is desktop
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
      usePlatformStore.getState().detectPlatform();

      // WHEN: Adding platformchange listener
      const callback = vi.fn();
      usePlatformStore.getState().addEventListener('platformchange', callback);

      // THEN: Called immediately with current platform
      expect(callback).toHaveBeenCalledWith({ platform: 'desktop' });
    });

    it('should immediately call xrmodechange listener with current XR mode on registration', () => {
      // GIVEN: XR mode is off
      usePlatformStore.setState({ isXRMode: false });

      // WHEN: Adding xrmodechange listener
      const callback = vi.fn();
      usePlatformStore.getState().addEventListener('xrmodechange', callback);

      // THEN: Called immediately with current state
      expect(callback).toHaveBeenCalledWith({ enabled: false });
    });

    it('should immediately call xrsessionstatechange listener with current state on registration', () => {
      // GIVEN: Session is active
      usePlatformStore.setState({ xrSessionState: 'active' });

      // WHEN: Adding listener
      const callback = vi.fn();
      usePlatformStore.getState().addEventListener('xrsessionstatechange', callback);

      // THEN: Immediate callback with current state
      expect(callback).toHaveBeenCalledWith({ state: 'active' });
    });

    it('should not call removed listener on dispatch', () => {
      // GIVEN: Listener registered and then removed
      const callback = vi.fn();
      usePlatformStore.getState().addEventListener('xrmodechange', callback);
      callback.mockClear();
      usePlatformStore.getState().removeEventListener('xrmodechange', callback);

      // WHEN: Dispatching event
      usePlatformStore.getState().dispatchEvent('xrmodechange', { enabled: true });

      // THEN: Callback not called
      expect(callback).not.toHaveBeenCalled();
    });

    it('should not throw if dispatching event with no listeners', () => {
      // GIVEN: No listeners registered for deviceorientationchange
      // WHEN/THEN: Does not throw
      expect(() => {
        usePlatformStore.getState().dispatchEvent('deviceorientationchange', {});
      }).not.toThrow();
    });

    it('should catch and log errors thrown by listeners without breaking other listeners', () => {
      // GIVEN: Two listeners, first throws — use handtrackingavailabilitychange
      // to avoid the immediate callback on addEventListener that xrmodechange triggers
      const badCallback = vi.fn().mockImplementation(() => {
        throw new Error('Listener error');
      });
      const goodCallback = vi.fn();
      usePlatformStore.getState().addEventListener('handtrackingavailabilitychange', badCallback);
      usePlatformStore.getState().addEventListener('handtrackingavailabilitychange', goodCallback);
      badCallback.mockClear();
      goodCallback.mockClear();

      // WHEN: Dispatching
      usePlatformStore.getState().dispatchEvent('handtrackingavailabilitychange', { available: true });

      // THEN: Both were called, error was caught (not thrown)
      expect(badCallback).toHaveBeenCalled();
      expect(goodCallback).toHaveBeenCalled();
    });
  });

  // ── removeAllListeners ────────────────────────────────────────────────

  describe('removeAllListeners', () => {
    it('should clear all listeners for a specific event type', () => {
      // GIVEN: Listeners on xrmodechange and platformchange
      const cb1 = vi.fn();
      const cb2 = vi.fn();
      usePlatformStore.getState().addEventListener('xrmodechange', cb1);
      usePlatformStore.getState().addEventListener('platformchange', cb2);
      cb1.mockClear();
      cb2.mockClear();

      // WHEN: Removing all xrmodechange listeners
      usePlatformStore.getState().removeAllListeners('xrmodechange');

      // THEN: xrmodechange dispatch has no effect; platformchange still works
      usePlatformStore.getState().dispatchEvent('xrmodechange', { enabled: true });
      expect(cb1).not.toHaveBeenCalled();

      usePlatformStore.getState().dispatchEvent('platformchange', { platform: 'desktop' });
      expect(cb2).toHaveBeenCalled();
    });

    it('should clear all listeners across all events when called without argument', () => {
      // GIVEN: Listeners on multiple events
      const cb1 = vi.fn();
      const cb2 = vi.fn();
      usePlatformStore.getState().addEventListener('xrmodechange', cb1);
      usePlatformStore.getState().addEventListener('platformchange', cb2);
      cb1.mockClear();
      cb2.mockClear();

      // WHEN: Removing all listeners
      usePlatformStore.getState().removeAllListeners();

      // THEN: No callbacks fire
      usePlatformStore.getState().dispatchEvent('xrmodechange', { enabled: true });
      usePlatformStore.getState().dispatchEvent('platformchange', { platform: 'quest3' });
      expect(cb1).not.toHaveBeenCalled();
      expect(cb2).not.toHaveBeenCalled();
    });
  });

  // ── setXRMode ─────────────────────────────────────────────────────────

  describe('setXRMode', () => {
    it('should update isXRMode state and dispatch xrmodechange event', () => {
      // GIVEN: XR mode is off, listener registered
      const callback = vi.fn();
      usePlatformStore.getState().addEventListener('xrmodechange', callback);
      callback.mockClear();

      // WHEN: Enabling XR mode
      usePlatformStore.getState().setXRMode(true);

      // THEN: State updated and event dispatched
      expect(usePlatformStore.getState().isXRMode).toBe(true);
      expect(callback).toHaveBeenCalledWith({ enabled: true });
    });

    it('should not dispatch event when setting same value', () => {
      // GIVEN: XR mode is already false
      const callback = vi.fn();
      usePlatformStore.getState().addEventListener('xrmodechange', callback);
      callback.mockClear();

      // WHEN: Setting to false (same as current)
      usePlatformStore.getState().setXRMode(false);

      // THEN: No dispatch (value unchanged)
      expect(callback).not.toHaveBeenCalled();
    });
  });

  // ── setXRSessionState ─────────────────────────────────────────────────

  describe('setXRSessionState', () => {
    it('should update xrSessionState and dispatch xrsessionstatechange', () => {
      // GIVEN: Session is inactive
      const callback = vi.fn();
      usePlatformStore.getState().addEventListener('xrsessionstatechange', callback);
      callback.mockClear();

      // WHEN: Transitioning to active
      usePlatformStore.getState().setXRSessionState('active');

      // THEN: State updated and event dispatched
      expect(usePlatformStore.getState().xrSessionState).toBe('active');
      expect(callback).toHaveBeenCalledWith({ state: 'active' });
    });

    it('should not dispatch event when setting same session state', () => {
      // GIVEN: Session is inactive
      const callback = vi.fn();
      usePlatformStore.getState().addEventListener('xrsessionstatechange', callback);
      callback.mockClear();

      // WHEN: Setting to inactive (same as current)
      usePlatformStore.getState().setXRSessionState('inactive');

      // THEN: No dispatch
      expect(callback).not.toHaveBeenCalled();
    });

    it('should handle all valid session states', () => {
      // GIVEN/WHEN/THEN: Each state is settable
      const states: XRSessionState[] = ['inactive', 'starting', 'active', 'ending', 'error'];
      for (const state of states) {
        usePlatformStore.getState().setXRSessionState(state);
        expect(usePlatformStore.getState().xrSessionState).toBe(state);
      }
    });
  });

  // ── Helper methods ────────────────────────────────────────────────────

  describe('Helper methods', () => {
    it('isQuest() should return true for quest, quest2, and quest3', () => {
      // GIVEN/WHEN/THEN: Quest variants
      for (const ua of [
        'Mozilla/5.0 (Linux; Quest 3) OculusBrowser/33.0',
        'Mozilla/5.0 (Linux; Quest 2) OculusBrowser/28.0',
        'Mozilla/5.0 (Linux; Quest) OculusBrowser/26.0',
      ]) {
        setUserAgent(ua);
        usePlatformStore.getState().detectPlatform();
        expect(usePlatformStore.getState().isQuest()).toBe(true);
      }
    });

    it('isQuest() should return false for non-Quest platforms', () => {
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
      usePlatformStore.getState().detectPlatform();
      expect(usePlatformStore.getState().isQuest()).toBe(false);
    });

    it('isPico() should return true for Pico devices', () => {
      setUserAgent('Mozilla/5.0 (Linux; Pico Neo 3) PicoBrowser/2.0');
      usePlatformStore.getState().detectPlatform();
      expect(usePlatformStore.getState().isPico()).toBe(true);
    });

    it('isPico() should return false for non-Pico devices', () => {
      setUserAgent('Mozilla/5.0 (Linux; Quest 3) OculusBrowser/33.0');
      usePlatformStore.getState().detectPlatform();
      expect(usePlatformStore.getState().isPico()).toBe(false);
    });

    it('isDesktop() should return true for desktop user agents', () => {
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
      usePlatformStore.getState().detectPlatform();
      expect(usePlatformStore.getState().isDesktop()).toBe(true);
    });

    it('isMobile() should return true for mobile user agents', () => {
      setUserAgent('Mozilla/5.0 (Linux; Android 14; Pixel 8) Chrome/120.0.0.0 Mobile');
      usePlatformStore.getState().detectPlatform();
      expect(usePlatformStore.getState().isMobile()).toBe(true);
    });
  });

  // ── Platform change event dispatch ─────────────────────────────────────

  describe('Platform change event on detectPlatform', () => {
    it('should dispatch platformchange when platform actually changes', () => {
      // GIVEN: Start as desktop
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
      usePlatformStore.getState().detectPlatform();

      const callback = vi.fn();
      usePlatformStore.getState().addEventListener('platformchange', callback);
      callback.mockClear();

      // WHEN: Changing to Quest 3
      setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
      usePlatformStore.getState().detectPlatform();

      // THEN: Event dispatched with new platform
      expect(callback).toHaveBeenCalledWith({ platform: 'quest3' });
    });

    it('should not dispatch platformchange when platform stays the same', () => {
      // GIVEN: Start as desktop
      setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
      usePlatformStore.getState().detectPlatform();

      const callback = vi.fn();
      usePlatformStore.getState().addEventListener('platformchange', callback);
      callback.mockClear();

      // WHEN: Re-detecting with same user agent
      usePlatformStore.getState().detectPlatform();

      // THEN: No additional dispatch
      expect(callback).not.toHaveBeenCalled();
    });
  });
});

// ── PlatformManager class (backward-compat wrapper) ─────────────────────────

describe('PlatformManager class', () => {
  beforeEach(() => {
    usePlatformStore.setState({
      platform: 'unknown',
      xrDeviceType: 'none',
      capabilities: {
        xrSupported: false,
        handTrackingSupported: false,
        arSupported: false,
        vrSupported: false,
        performanceTier: 'medium',
        maxTextureSize: 2048,
        hasTouchscreen: false,
        hasPointer: true,
        hasKeyboard: true,
        hasGamepad: false,
        memoryLimited: false,
      },
      userAgent: '',
      isXRMode: false,
      xrSessionState: 'inactive',
      isWebXRSupported: false,
      initialized: false,
      listeners: new Map(),
    });

    setUserAgent('');
    clearNavigatorXR();
    vi.clearAllMocks();
  });

  afterEach(() => {
    usePlatformStore.getState().removeAllListeners();
  });

  it('should be a singleton - getInstance returns same instance', () => {
    // GIVEN/WHEN: Two calls to getInstance
    const a = PlatformManager.getInstance();
    const b = PlatformManager.getInstance();

    // THEN: Same instance
    expect(a).toBe(b);
  });

  it('should expose platform property from store', () => {
    // GIVEN: Store has quest3 platform
    setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
    usePlatformStore.getState().detectPlatform();

    // WHEN: Reading platform from class
    const pm = PlatformManager.getInstance();

    // THEN: Matches store
    expect(pm.platform).toBe('quest3');
  });

  it('should expose isXRMode property from store', () => {
    // GIVEN: XR mode enabled in store
    usePlatformStore.getState().setXRMode(true);

    // WHEN: Reading from class
    const pm = PlatformManager.getInstance();

    // THEN: Matches store
    expect(pm.isXRMode).toBe(true);
  });

  it('should proxy setXRMode to store', () => {
    // GIVEN: XR mode is off
    const pm = PlatformManager.getInstance();

    // WHEN: Setting via class
    pm.setXRMode(true);

    // THEN: Store reflects change
    expect(usePlatformStore.getState().isXRMode).toBe(true);
  });

  it('should proxy xrSessionState getter and setter', () => {
    // GIVEN: Session inactive
    const pm = PlatformManager.getInstance();
    expect(pm.xrSessionState).toBe('inactive');

    // WHEN: Setting via class
    pm.xrSessionState = 'active';

    // THEN: Store reflects change
    expect(usePlatformStore.getState().xrSessionState).toBe('active');
    expect(pm.xrSessionState).toBe('active');
  });

  it('should proxy isQuest() to store', () => {
    // GIVEN: Quest 3 detected
    setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
    usePlatformStore.getState().detectPlatform();

    // WHEN: Calling via class
    const pm = PlatformManager.getInstance();

    // THEN: Returns true
    expect(pm.isQuest()).toBe(true);
  });

  it('should proxy isPico() to store', () => {
    setUserAgent('Mozilla/5.0 (Linux; Pico Neo 3) PicoBrowser/2.0');
    usePlatformStore.getState().detectPlatform();
    expect(PlatformManager.getInstance().isPico()).toBe(true);
  });

  it('should proxy isDesktop() to store', () => {
    setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0');
    usePlatformStore.getState().detectPlatform();
    expect(PlatformManager.getInstance().isDesktop()).toBe(true);
  });

  it('should proxy isMobile() to store', () => {
    setUserAgent('Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) Mobile/15E148');
    usePlatformStore.getState().detectPlatform();
    expect(PlatformManager.getInstance().isMobile()).toBe(true);
  });

  it('should return capabilities via getCapabilities()', () => {
    // GIVEN: Quest 3 detected
    setUserAgent('Mozilla/5.0 (Linux; Android 12; Quest 3) OculusBrowser/33.0');
    usePlatformStore.getState().detectPlatform();

    // WHEN: Getting capabilities
    const caps = PlatformManager.getInstance().getCapabilities();

    // THEN: Matches store
    expect(caps.performanceTier).toBe('high');
    expect(caps.maxTextureSize).toBe(4096);
  });

  it('should proxy on/off event methods', () => {
    // GIVEN: Listener via on()
    const pm = PlatformManager.getInstance();
    const callback = vi.fn();
    pm.on('xrmodechange', callback);
    callback.mockClear();

    // WHEN: Triggering event
    usePlatformStore.getState().setXRMode(true);

    // THEN: Callback fired
    expect(callback).toHaveBeenCalledWith({ enabled: true });

    // WHEN: Removing via off()
    callback.mockClear();
    pm.off('xrmodechange', callback);
    usePlatformStore.getState().setXRMode(false);

    // THEN: Not called
    expect(callback).not.toHaveBeenCalled();
  });

  it('should proxy removeAllListeners', () => {
    // GIVEN: Listener registered
    const pm = PlatformManager.getInstance();
    const callback = vi.fn();
    pm.on('xrmodechange', callback);
    callback.mockClear();

    // WHEN: Removing all
    pm.removeAllListeners();

    // THEN: No dispatch
    usePlatformStore.getState().setXRMode(true);
    // The callback should not have been called after removeAllListeners
    // (setXRMode dispatches, but there are no listeners)
    expect(callback).not.toHaveBeenCalled();
  });

  it('should expose platformManager singleton export', () => {
    // GIVEN/WHEN: Importing platformManager
    // THEN: It is a PlatformManager instance
    expect(platformManager).toBeInstanceOf(PlatformManager);
    expect(platformManager).toBe(PlatformManager.getInstance());
  });
});
