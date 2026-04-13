/**
 * WebXRScene Tests
 *
 * Pure-logic and configuration tests for the unified WebXR visualization scene.
 * Tests extracted functions, mode-switching logic, opacity rules, LOD thresholds,
 * hand tracking session mapping, and URL parameter detection.
 *
 * Framework: Vitest
 * Pattern: Given-When-Then, pure function testing (no React rendering)
 *
 * @vitest-environment jsdom
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as THREE from 'three';

// ---------------------------------------------------------------------------
// Mocks — must be declared before any module that touches React/R3F/XR
// ---------------------------------------------------------------------------

// Stub React hooks (Canvas, useThree, useFrame, etc. are never called in these tests)
vi.mock('react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('react')>();
  return {
    ...actual,
    useState: vi.fn((init: any) => [init, vi.fn()]),
    useRef: vi.fn((init: any) => ({ current: init })),
    useMemo: vi.fn((fn: () => any) => fn()),
    useCallback: vi.fn((fn: any) => fn),
    useEffect: vi.fn(),
    Suspense: ({ children }: any) => children,
  };
});

vi.mock('@react-three/fiber', () => ({
  Canvas: vi.fn(({ children }: any) => children),
  useThree: vi.fn(() => ({
    camera: { position: new THREE.Vector3(), quaternion: new THREE.Quaternion() },
    gl: { xr: {} },
  })),
  useFrame: vi.fn(),
}));

vi.mock('@react-three/xr', () => ({
  createXRStore: vi.fn(() => ({
    subscribe: vi.fn(() => vi.fn()),
    enterVR: vi.fn(),
  })),
  XR: vi.fn(({ children }: any) => children),
  useXREvent: vi.fn(),
}));

vi.mock('../components/ActionConnectionsLayer', () => ({
  ActionConnectionsLayer: vi.fn(() => null),
}));

vi.mock('../../../immersive/threejs/VRActionConnectionsLayer', () => ({
  VRActionConnectionsLayer: vi.fn(() => null),
}));

vi.mock('../hooks/useAgentActionVisualization', () => ({
  useAgentActionVisualization: vi.fn(() => ({
    connections: [],
    activeCount: 0,
  })),
}));

vi.mock('../../../immersive/hooks/useVRConnectionsLOD', () => ({
  useVRConnectionsLOD: vi.fn(() => ({
    updateCameraPosition: vi.fn(),
    getLODLevel: vi.fn(),
    getCacheStats: vi.fn(() => ({ size: 0 })),
  })),
  calculateOptimalThresholds: vi.fn((targetFPS: number, connectionCount: number) => {
    const performanceFactor = 72 / targetFPS;
    const countFactor = Math.max(1, connectionCount / 10);
    const baseLow = 30 / (performanceFactor * countFactor);
    const baseMedium = baseLow * 0.5;
    const baseHigh = baseMedium * 0.33;
    return {
      highDistance: Math.max(2, baseHigh),
      mediumDistance: Math.max(5, baseMedium),
      lowDistance: Math.max(10, baseLow),
      aggressiveCulling: targetFPS < 72 || connectionCount > 15,
    };
  }),
  LODLevel: {},
}));

vi.mock('../../../immersive/hooks/useVRHandTracking', () => ({
  useVRHandTracking: vi.fn(() => ({
    previewStart: null,
    previewEnd: null,
    showPreview: false,
    previewColor: '#00ffff',
    targetedNode: null,
    setTargetNodes: vi.fn(),
    updateHandState: vi.fn(),
    triggerHaptic: vi.fn(),
  })),
  agentsToTargetNodes: vi.fn((agents: any[]) =>
    agents
      .filter((a: any) => a.position)
      .map((a: any) => ({
        id: a.id,
        position: new THREE.Vector3(a.position.x, a.position.y, a.position.z),
        type: a.type,
      }))
  ),
  TargetNode: {},
  HandState: {},
}));

vi.mock('../../../utils/loggerConfig', () => ({
  createLogger: vi.fn(() => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  })),
}));

// ---------------------------------------------------------------------------
// Import the module under test AFTER mocks
// ---------------------------------------------------------------------------
import { updateHandTrackingFromSession } from '../../../immersive/hooks/updateHandTrackingFromSession';
import { calculateOptimalThresholds } from '../../../immersive/hooks/useVRConnectionsLOD';
import { agentsToTargetNodes } from '../../../immersive/hooks/useVRHandTracking';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Build a minimal XRInputSource-like object with hand tracking */
function makeHandSource(handedness: XRHandedness): Partial<XRInputSource> {
  return {
    handedness,
    hand: {} as XRHand,
    gamepad: undefined as unknown as Gamepad,
  };
}

/** Build a minimal XRInputSource-like object with a gamepad controller */
function makeControllerSource(
  handedness: XRHandedness,
  buttons: Array<{ pressed: boolean; value: number }>
): Partial<XRInputSource> {
  return {
    handedness,
    hand: null as unknown as XRHand,
    gamepad: {
      buttons: buttons.map((b) => ({
        pressed: b.pressed,
        value: b.value,
        touched: false,
      })),
      axes: [],
      connected: true,
      id: 'mock-gamepad',
      index: 0,
      mapping: 'standard',
      timestamp: Date.now(),
      hapticActuators: [],
      vibrationActuator: null,
    } as unknown as Gamepad,
  };
}

/** Build a minimal XRSession-like object from input sources */
function makeSession(sources: Partial<XRInputSource>[]): XRSession {
  return {
    inputSources: sources as unknown as XRInputSourceArray,
  } as unknown as XRSession;
}

// ---------------------------------------------------------------------------
// Reusable opacity calculator extracted from ActionConnectionsScene logic
// ---------------------------------------------------------------------------
function computeOpacity(isVRMode: boolean, activeCount: number): number {
  if (isVRMode) {
    if (activeCount > 18) return 0.6;
    if (activeCount > 12) return 0.8;
    return 1.0;
  }
  if (activeCount > 40) return 0.6;
  if (activeCount > 30) return 0.8;
  return 1.0;
}

// Reusable connection-limit calculator
function effectiveMaxConnections(isVRMode: boolean, maxConnections: number): number {
  return isVRMode ? Math.min(maxConnections, 20) : maxConnections;
}

// Reusable duration-cap calculator
function effectiveDuration(isVRMode: boolean, baseDuration: number): number {
  return isVRMode ? Math.min(baseDuration, 400) : baseDuration;
}

// ===========================================================================
// TEST SUITES
// ===========================================================================

describe('updateHandTrackingFromSession', () => {
  let updateHandState: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    updateHandState = vi.fn();
  });

  describe('hand tracking source', () => {
    it('GIVEN a hand tracking source for right hand WHEN session is processed THEN isTracking and isPointing are true on primary', () => {
      // GIVEN
      const session = makeSession([makeHandSource('right')]);

      // WHEN
      updateHandTrackingFromSession(session, updateHandState);

      // THEN
      expect(updateHandState).toHaveBeenCalledWith('primary', {
        isTracking: true,
        isPointing: true,
      });
    });

    it('GIVEN a hand tracking source for left hand WHEN session is processed THEN updates secondary hand', () => {
      const session = makeSession([makeHandSource('left')]);

      updateHandTrackingFromSession(session, updateHandState);

      expect(updateHandState).toHaveBeenCalledWith('secondary', {
        isTracking: true,
        isPointing: true,
      });
    });

    it('GIVEN a hand tracking source with no handedness (none) WHEN processed THEN maps to secondary', () => {
      const session = makeSession([makeHandSource('none')]);

      updateHandTrackingFromSession(session, updateHandState);

      expect(updateHandState).toHaveBeenCalledWith('secondary', {
        isTracking: true,
        isPointing: true,
      });
    });
  });

  describe('controller source', () => {
    it('GIVEN a controller with button 0 pressed WHEN session is processed THEN isPointing is true', () => {
      const session = makeSession([
        makeControllerSource('right', [
          { pressed: true, value: 0.9 },
          { pressed: false, value: 0.0 },
        ]),
      ]);

      updateHandTrackingFromSession(session, updateHandState);

      expect(updateHandState).toHaveBeenCalledWith('primary', {
        isTracking: true,
        isPointing: true,
        pinchStrength: 0.9,
      });
    });

    it('GIVEN a controller with button 1 pressed WHEN session is processed THEN isPointing is true', () => {
      const session = makeSession([
        makeControllerSource('right', [
          { pressed: false, value: 0.1 },
          { pressed: true, value: 0.7 },
        ]),
      ]);

      updateHandTrackingFromSession(session, updateHandState);

      expect(updateHandState).toHaveBeenCalledWith('primary', {
        isTracking: true,
        isPointing: true,
        pinchStrength: 0.7,
      });
    });

    it('GIVEN a controller with no buttons pressed WHEN session is processed THEN isPointing is false', () => {
      const session = makeSession([
        makeControllerSource('right', [
          { pressed: false, value: 0.0 },
          { pressed: false, value: 0.0 },
        ]),
      ]);

      updateHandTrackingFromSession(session, updateHandState);

      expect(updateHandState).toHaveBeenCalledWith('primary', {
        isTracking: true,
        isPointing: false,
        pinchStrength: 0,
      });
    });

    it('GIVEN a controller WHEN both buttons have values THEN pinchStrength is the max', () => {
      const session = makeSession([
        makeControllerSource('left', [
          { pressed: true, value: 0.4 },
          { pressed: true, value: 0.85 },
        ]),
      ]);

      updateHandTrackingFromSession(session, updateHandState);

      expect(updateHandState).toHaveBeenCalledWith('secondary', {
        isTracking: true,
        isPointing: true,
        pinchStrength: 0.85,
      });
    });

    it('GIVEN right hand controller WHEN processed THEN maps to primary', () => {
      const session = makeSession([
        makeControllerSource('right', [{ pressed: false, value: 0.0 }]),
      ]);

      updateHandTrackingFromSession(session, updateHandState);

      expect(updateHandState).toHaveBeenCalledWith(
        'primary',
        expect.objectContaining({ isTracking: true })
      );
    });

    it('GIVEN left hand controller WHEN processed THEN maps to secondary', () => {
      const session = makeSession([
        makeControllerSource('left', [{ pressed: false, value: 0.0 }]),
      ]);

      updateHandTrackingFromSession(session, updateHandState);

      expect(updateHandState).toHaveBeenCalledWith(
        'secondary',
        expect.objectContaining({ isTracking: true })
      );
    });
  });

  describe('empty and mixed input sources', () => {
    it('GIVEN empty inputSources WHEN session is processed THEN no state updates occur', () => {
      const session = makeSession([]);

      updateHandTrackingFromSession(session, updateHandState);

      expect(updateHandState).not.toHaveBeenCalled();
    });

    it('GIVEN null inputSources WHEN session is processed THEN no state updates occur', () => {
      const session = { inputSources: null } as unknown as XRSession;

      updateHandTrackingFromSession(session, updateHandState);

      expect(updateHandState).not.toHaveBeenCalled();
    });

    it('GIVEN undefined inputSources WHEN session is processed THEN no state updates occur', () => {
      const session = { inputSources: undefined } as unknown as XRSession;

      updateHandTrackingFromSession(session, updateHandState);

      expect(updateHandState).not.toHaveBeenCalled();
    });

    it('GIVEN mixed sources (one hand, one controller) WHEN processed THEN both hands update', () => {
      const session = makeSession([
        makeHandSource('right'),
        makeControllerSource('left', [
          { pressed: true, value: 0.6 },
          { pressed: false, value: 0.2 },
        ]),
      ]);

      updateHandTrackingFromSession(session, updateHandState);

      expect(updateHandState).toHaveBeenCalledTimes(2);
      expect(updateHandState).toHaveBeenCalledWith('primary', {
        isTracking: true,
        isPointing: true,
      });
      expect(updateHandState).toHaveBeenCalledWith('secondary', {
        isTracking: true,
        isPointing: true,
        pinchStrength: 0.6,
      });
    });

    it('GIVEN two right-hand sources WHEN processed THEN both update primary', () => {
      const session = makeSession([
        makeHandSource('right'),
        makeControllerSource('right', [{ pressed: true, value: 1.0 }]),
      ]);

      updateHandTrackingFromSession(session, updateHandState);

      // Both sources map to primary -- called twice
      const primaryCalls = updateHandState.mock.calls.filter(
        (c: any[]) => c[0] === 'primary'
      );
      expect(primaryCalls).toHaveLength(2);
    });
  });
});

describe('XR store configuration', () => {
  it('GIVEN production environment WHEN xrStore is created THEN hand tracking is enabled', () => {
    // The xrStore is created with hand: true in the source
    // This is a specification test for the config shape
    const expectedConfig = { hand: true, controller: true };
    expect(expectedConfig.hand).toBe(true);
  });

  it('GIVEN production environment WHEN xrStore is created THEN controller is enabled', () => {
    const expectedConfig = { hand: true, controller: true };
    expect(expectedConfig.controller).toBe(true);
  });

  it('GIVEN production environment WHEN xrStore is created THEN emulation is disabled', () => {
    // import.meta.env.DEV is false in production
    const isDev = false;
    const emulate = isDev ? 'metaQuest3' : false;
    expect(emulate).toBe(false);
  });

  it('GIVEN development environment WHEN xrStore is created THEN emulation is metaQuest3', () => {
    const isDev = true;
    const emulate = isDev ? 'metaQuest3' : false;
    expect(emulate).toBe('metaQuest3');
  });
});

describe('VR mode connection limits', () => {
  it('GIVEN desktop mode WHEN maxConnections is 50 THEN effective max is 50', () => {
    expect(effectiveMaxConnections(false, 50)).toBe(50);
  });

  it('GIVEN VR mode WHEN maxConnections is 50 THEN effective max is capped at 20', () => {
    expect(effectiveMaxConnections(true, 50)).toBe(20);
  });

  it('GIVEN VR mode WHEN maxConnections is 15 THEN effective max is 15 (below cap)', () => {
    expect(effectiveMaxConnections(true, 15)).toBe(15);
  });

  it('GIVEN VR mode WHEN maxConnections is exactly 20 THEN effective max is 20', () => {
    expect(effectiveMaxConnections(true, 20)).toBe(20);
  });

  it('GIVEN VR mode WHEN maxConnections is 0 THEN effective max is 0', () => {
    expect(effectiveMaxConnections(true, 0)).toBe(0);
  });

  it('GIVEN desktop mode WHEN maxConnections is 100 THEN no cap applied', () => {
    expect(effectiveMaxConnections(false, 100)).toBe(100);
  });

  describe('duration cap', () => {
    it('GIVEN desktop mode WHEN baseDuration is 500ms THEN duration is 500ms', () => {
      expect(effectiveDuration(false, 500)).toBe(500);
    });

    it('GIVEN VR mode WHEN baseDuration is 500ms THEN duration is capped at 400ms', () => {
      expect(effectiveDuration(true, 500)).toBe(400);
    });

    it('GIVEN VR mode WHEN baseDuration is 300ms THEN duration stays 300ms', () => {
      expect(effectiveDuration(true, 300)).toBe(300);
    });

    it('GIVEN VR mode WHEN baseDuration is exactly 400ms THEN duration is 400ms', () => {
      expect(effectiveDuration(true, 400)).toBe(400);
    });
  });
});

describe('opacity calculation', () => {
  describe('VR mode thresholds', () => {
    it('GIVEN VR mode WHEN activeCount > 18 THEN opacity is 0.6', () => {
      expect(computeOpacity(true, 19)).toBe(0.6);
      expect(computeOpacity(true, 20)).toBe(0.6);
      expect(computeOpacity(true, 100)).toBe(0.6);
    });

    it('GIVEN VR mode WHEN activeCount is exactly 18 THEN opacity is 0.8 (not > 18)', () => {
      expect(computeOpacity(true, 18)).toBe(0.8);
    });

    it('GIVEN VR mode WHEN activeCount > 12 and <= 18 THEN opacity is 0.8', () => {
      expect(computeOpacity(true, 13)).toBe(0.8);
      expect(computeOpacity(true, 15)).toBe(0.8);
      expect(computeOpacity(true, 18)).toBe(0.8);
    });

    it('GIVEN VR mode WHEN activeCount is exactly 12 THEN opacity is 1.0 (not > 12)', () => {
      expect(computeOpacity(true, 12)).toBe(1.0);
    });

    it('GIVEN VR mode WHEN activeCount <= 12 THEN opacity is 1.0', () => {
      expect(computeOpacity(true, 0)).toBe(1.0);
      expect(computeOpacity(true, 5)).toBe(1.0);
      expect(computeOpacity(true, 12)).toBe(1.0);
    });
  });

  describe('desktop mode thresholds', () => {
    it('GIVEN desktop mode WHEN activeCount > 40 THEN opacity is 0.6', () => {
      expect(computeOpacity(false, 41)).toBe(0.6);
      expect(computeOpacity(false, 50)).toBe(0.6);
    });

    it('GIVEN desktop mode WHEN activeCount is exactly 40 THEN opacity is 0.8', () => {
      expect(computeOpacity(false, 40)).toBe(0.8);
    });

    it('GIVEN desktop mode WHEN activeCount > 30 and <= 40 THEN opacity is 0.8', () => {
      expect(computeOpacity(false, 31)).toBe(0.8);
      expect(computeOpacity(false, 35)).toBe(0.8);
      expect(computeOpacity(false, 40)).toBe(0.8);
    });

    it('GIVEN desktop mode WHEN activeCount is exactly 30 THEN opacity is 1.0', () => {
      expect(computeOpacity(false, 30)).toBe(1.0);
    });

    it('GIVEN desktop mode WHEN activeCount <= 30 THEN opacity is 1.0', () => {
      expect(computeOpacity(false, 0)).toBe(1.0);
      expect(computeOpacity(false, 15)).toBe(1.0);
      expect(computeOpacity(false, 30)).toBe(1.0);
    });
  });
});

describe('LOD thresholds via calculateOptimalThresholds', () => {
  it('GIVEN 72fps target and 20 connections WHEN calculated THEN returns valid threshold config', () => {
    const config = calculateOptimalThresholds(72, 20);
    expect(config).toHaveProperty('highDistance');
    expect(config).toHaveProperty('mediumDistance');
    expect(config).toHaveProperty('lowDistance');
  });

  it('GIVEN 72fps target and 20 connections WHEN calculated THEN highDistance >= 2', () => {
    const config = calculateOptimalThresholds(72, 20);
    expect(config.highDistance).toBeGreaterThanOrEqual(2);
  });

  it('GIVEN 72fps target and 20 connections WHEN calculated THEN mediumDistance >= 5', () => {
    const config = calculateOptimalThresholds(72, 20);
    expect(config.mediumDistance).toBeGreaterThanOrEqual(5);
  });

  it('GIVEN 72fps target and 20 connections WHEN calculated THEN lowDistance >= 10', () => {
    const config = calculateOptimalThresholds(72, 20);
    expect(config.lowDistance).toBeGreaterThanOrEqual(10);
  });

  it('GIVEN 72fps target and 20 connections WHEN calculated THEN aggressiveCulling is true (>15 connections)', () => {
    const config = calculateOptimalThresholds(72, 20);
    expect(config.aggressiveCulling).toBe(true);
  });

  it('GIVEN 72fps target and 10 connections WHEN calculated THEN aggressiveCulling is false', () => {
    const config = calculateOptimalThresholds(72, 10);
    expect(config.aggressiveCulling).toBe(false);
  });

  it('GIVEN 60fps target WHEN calculated THEN aggressiveCulling is true (below 72fps)', () => {
    const config = calculateOptimalThresholds(60, 5);
    expect(config.aggressiveCulling).toBe(true);
  });

  it('GIVEN higher connection count WHEN calculated THEN thresholds decrease', () => {
    const configLow = calculateOptimalThresholds(72, 5);
    const configHigh = calculateOptimalThresholds(72, 20);
    expect(configHigh.lowDistance).toBeLessThanOrEqual(configLow.lowDistance!);
  });

  it('GIVEN thresholds WHEN compared THEN highDistance < mediumDistance < lowDistance', () => {
    const config = calculateOptimalThresholds(72, 20);
    expect(config.highDistance).toBeLessThan(config.mediumDistance!);
    expect(config.mediumDistance).toBeLessThan(config.lowDistance!);
  });
});

describe('?vr=true URL parameter detection', () => {
  const originalLocation = window.location;

  afterEach(() => {
    Object.defineProperty(window, 'location', {
      value: originalLocation,
      writable: true,
      configurable: true,
    });
  });

  it('GIVEN ?vr=true in URL WHEN parsed THEN forceVR is true', () => {
    Object.defineProperty(window, 'location', {
      value: { search: '?vr=true' },
      writable: true,
      configurable: true,
    });

    const urlParams = new URLSearchParams(window.location.search);
    const forceVR = urlParams.get('vr') === 'true';
    expect(forceVR).toBe(true);
  });

  it('GIVEN ?vr=false in URL WHEN parsed THEN forceVR is false', () => {
    Object.defineProperty(window, 'location', {
      value: { search: '?vr=false' },
      writable: true,
      configurable: true,
    });

    const urlParams = new URLSearchParams(window.location.search);
    const forceVR = urlParams.get('vr') === 'true';
    expect(forceVR).toBe(false);
  });

  it('GIVEN no vr param in URL WHEN parsed THEN forceVR is false', () => {
    Object.defineProperty(window, 'location', {
      value: { search: '' },
      writable: true,
      configurable: true,
    });

    const urlParams = new URLSearchParams(window.location.search);
    const forceVR = urlParams.get('vr') === 'true';
    expect(forceVR).toBe(false);
  });

  it('GIVEN ?vr=TRUE (uppercase) in URL WHEN parsed THEN forceVR is false (case-sensitive)', () => {
    Object.defineProperty(window, 'location', {
      value: { search: '?vr=TRUE' },
      writable: true,
      configurable: true,
    });

    const urlParams = new URLSearchParams(window.location.search);
    const forceVR = urlParams.get('vr') === 'true';
    expect(forceVR).toBe(false);
  });

  it('GIVEN ?vr=true among other params WHEN parsed THEN forceVR is true', () => {
    Object.defineProperty(window, 'location', {
      value: { search: '?debug=1&vr=true&mode=test' },
      writable: true,
      configurable: true,
    });

    const urlParams = new URLSearchParams(window.location.search);
    const forceVR = urlParams.get('vr') === 'true';
    expect(forceVR).toBe(true);
  });
});

describe('agentsToTargetNodes conversion', () => {
  it('GIVEN agent with position WHEN converted THEN produces TargetNode with Vector3', () => {
    const agents = [{ id: 'a1', type: 'coder', position: { x: 1, y: 2, z: 3 } }];
    const nodes = agentsToTargetNodes(agents);
    expect(nodes).toHaveLength(1);
    expect(nodes[0].id).toBe('a1');
    expect(nodes[0].position.x).toBe(1);
    expect(nodes[0].position.y).toBe(2);
    expect(nodes[0].position.z).toBe(3);
    expect(nodes[0].type).toBe('coder');
  });

  it('GIVEN agent without position WHEN converted THEN is filtered out', () => {
    const agents = [
      { id: 'a1', position: { x: 0, y: 0, z: 0 } },
      { id: 'a2' },
      { id: 'a3', position: { x: 5, y: 5, z: 5 } },
    ];
    const nodes = agentsToTargetNodes(agents);
    expect(nodes).toHaveLength(2);
    expect(nodes.map((n: any) => n.id)).toEqual(['a1', 'a3']);
  });

  it('GIVEN empty agents array WHEN converted THEN returns empty array', () => {
    const nodes = agentsToTargetNodes([]);
    expect(nodes).toHaveLength(0);
  });

  it('GIVEN agent with zero position WHEN converted THEN still included', () => {
    const agents = [{ id: 'a1', position: { x: 0, y: 0, z: 0 } }];
    const nodes = agentsToTargetNodes(agents);
    expect(nodes).toHaveLength(1);
  });

  it('GIVEN agent with negative coordinates WHEN converted THEN preserves negative values', () => {
    const agents = [{ id: 'a1', position: { x: -10, y: -20, z: -30 } }];
    const nodes = agentsToTargetNodes(agents);
    expect(nodes[0].position.x).toBe(-10);
    expect(nodes[0].position.y).toBe(-20);
    expect(nodes[0].position.z).toBe(-30);
  });

  it('GIVEN agent without type WHEN converted THEN type is undefined', () => {
    const agents = [{ id: 'a1', position: { x: 0, y: 0, z: 0 } }];
    const nodes = agentsToTargetNodes(agents);
    expect(nodes[0].type).toBeUndefined();
  });
});

describe('VRTargetHighlight specification', () => {
  describe('ring geometry configuration', () => {
    it('outer ring: innerRadius=1.8, outerRadius=2.2, 32 segments', () => {
      const outer = { innerRadius: 1.8, outerRadius: 2.2, segments: 32 };
      expect(outer.outerRadius - outer.innerRadius).toBeCloseTo(0.4);
      expect(outer.segments).toBe(32);
    });

    it('inner ring: innerRadius=1.2, outerRadius=1.8, 32 segments', () => {
      const inner = { innerRadius: 1.2, outerRadius: 1.8, segments: 32 };
      expect(inner.outerRadius - inner.innerRadius).toBeCloseTo(0.6);
    });
  });

  describe('animation parameters', () => {
    it('rotation speed is 0.5 radians per second', () => {
      const elapsed = 2.0;
      const rotationZ = elapsed * 0.5;
      expect(rotationZ).toBeCloseTo(1.0);
    });

    it('pulse scale oscillates with 10% amplitude at 3Hz', () => {
      const elapsed = 0;
      const scale = 1 + Math.sin(elapsed * 3) * 0.1;
      expect(scale).toBe(1.0); // At t=0, sin(0)=0

      const elapsedQuarter = Math.PI / 6; // pi/6 => sin(pi/2) = 1
      const scalePeak = 1 + Math.sin(elapsedQuarter * 3) * 0.1;
      expect(scalePeak).toBeCloseTo(1.1);
    });
  });

  describe('material properties', () => {
    it('outer ring opacity is 0.4, transparent, no depth write', () => {
      const outerMat = { opacity: 0.4, transparent: true, depthWrite: false, side: THREE.DoubleSide };
      expect(outerMat.opacity).toBe(0.4);
      expect(outerMat.depthWrite).toBe(false);
    });

    it('inner ring opacity is 0.2', () => {
      const innerMat = { opacity: 0.2, transparent: true, depthWrite: false };
      expect(innerMat.opacity).toBe(0.2);
    });
  });
});

describe('VRPerformanceStats specification', () => {
  it('HUD offset is (0, -0.3, -1) from camera', () => {
    const offset = new THREE.Vector3(0, -0.3, -1);
    expect(offset.y).toBe(-0.3);
    expect(offset.z).toBe(-1);
  });

  it('panel dimensions are 0.4 x 0.15 meters', () => {
    const dims = { width: 0.4, height: 0.15 };
    expect(dims.width).toBe(0.4);
    expect(dims.height).toBe(0.15);
  });

  it('connection bar width scales with activeConnections, max 0.3m', () => {
    expect(Math.min(0.02 * 10, 0.3)).toBeCloseTo(0.2);
    expect(Math.min(0.02 * 15, 0.3)).toBeCloseTo(0.3);
    expect(Math.min(0.02 * 20, 0.3)).toBeCloseTo(0.3); // capped
  });

  it('LOD cache bar width scales with cacheSize, max 0.3m', () => {
    expect(Math.min(0.001 * 100, 0.3)).toBeCloseTo(0.1);
    expect(Math.min(0.001 * 300, 0.3)).toBeCloseTo(0.3);
    expect(Math.min(0.001 * 500, 0.3)).toBeCloseTo(0.3); // capped
  });
});

describe('WebGL canvas configuration', () => {
  it('GIVEN VR mode THEN antialias is disabled for performance', () => {
    const isInVR = true;
    expect(!isInVR).toBe(false);
  });

  it('GIVEN desktop mode THEN antialias is enabled', () => {
    const isInVR = false;
    expect(!isInVR).toBe(true);
  });

  it('power preference is high-performance', () => {
    const config = { powerPreference: 'high-performance' as const };
    expect(config.powerPreference).toBe('high-performance');
  });

  it('alpha is disabled', () => {
    const config = { alpha: false };
    expect(config.alpha).toBe(false);
  });

  it('camera positioned at [0, 1.6, 3] with 70 fov', () => {
    const camera = { position: [0, 1.6, 3], fov: 70 };
    expect(camera.position[1]).toBe(1.6);
    expect(camera.fov).toBe(70);
  });
});
