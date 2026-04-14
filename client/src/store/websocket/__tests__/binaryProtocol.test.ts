/**
 * binaryProtocol.test.ts -- Unit tests for client-side binary protocol validation
 *
 * Covers: validateBinaryData, resetBinaryState, VALID_VERSIONS constant (V2 removed).
 * Uses Given-When-Then structure throughout.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

// ── Mocks ────────────────────────────────────────────────────────────────────
// Mock all transitive imports so the module loads without side-effect errors.

vi.mock('../../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
  createErrorMetadata: vi.fn((e: unknown) => e),
}));

vi.mock('../../../utils/clientDebugState', () => ({
  debugState: {
    isDataDebugEnabled: vi.fn(() => false),
  },
}));

vi.mock('../../settingsStore', () => ({
  useSettingsStore: {
    getState: vi.fn(() => ({
      get: vi.fn(() => 'knowledge_graph'),
    })),
  },
}));

vi.mock('../../../features/graph/managers/graphDataManager', () => ({
  graphDataManager: {
    getGraphType: vi.fn(() => 'knowledge_graph'),
    updateNodePositions: vi.fn(),
  },
}));

vi.mock('../../../types/binaryProtocol', () => ({
  parseBinaryNodeData: vi.fn(() => []),
  parseBinaryFrameData: vi.fn(() => ({ nodes: [], broadcastSequence: 0 })),
  isAgentNode: vi.fn(() => false),
  getNodeType: vi.fn(() => 0),
  getActualNodeId: vi.fn((id: number) => id),
  NodeType: { Unknown: 0, Regular: 1, Agent: 2 },
  PROTOCOL_V3: 3,
  PROTOCOL_V5: 5,
}));

vi.mock('../../../utils/BatchQueue', () => ({
  NodePositionBatchQueue: vi.fn().mockImplementation(() => ({
    enqueuePositionUpdate: vi.fn(),
    flush: vi.fn().mockResolvedValue(undefined),
    destroy: vi.fn(),
    getMetrics: vi.fn(() => null),
  })),
  createWebSocketBatchProcessor: vi.fn(() => ({
    processBatch: vi.fn(),
    onError: vi.fn(),
    onSuccess: vi.fn(),
  })),
}));

vi.mock('../../../utils/validation', () => ({
  validateNodePositions: vi.fn(() => ({ valid: true, errors: [] })),
  createValidationMiddleware: vi.fn(() => (batch: unknown[]) => batch),
}));

vi.mock('../../../services/BinaryWebSocketProtocol', () => ({
  binaryProtocol: {
    parseHeader: vi.fn(() => null),
    extractPayload: vi.fn(() => new ArrayBuffer(0)),
    createBroadcastAck: vi.fn(() => new ArrayBuffer(0)),
    decodeAgentActions: vi.fn(() => []),
  },
  MessageType: {
    GRAPH_UPDATE: 0x01,
    VOICE_DATA: 0x02,
    POSITION_UPDATE: 0x10,
    AGENT_POSITIONS: 0x11,
    AGENT_ACTION: 0x23,
  },
  GraphTypeFlag: {
    KNOWLEDGE_GRAPH: 0,
    ONTOLOGY: 1,
  },
}));

vi.mock('../types', () => ({}));

vi.mock('../connectionManager', () => ({
  emit: vi.fn(),
  notifyBinaryMessageHandlers: vi.fn(),
}));

// ── Import under test (after mocks) ─────────────────────────────────────────
import { validateBinaryData, resetBinaryState } from '../binaryProtocol';

// ── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Build an ArrayBuffer of the given size with the first byte set to `version`.
 */
function createBufferWithVersion(version: number, totalBytes: number): ArrayBuffer {
  const buffer = new ArrayBuffer(totalBytes);
  new DataView(buffer).setUint8(0, version);
  return buffer;
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe('validateBinaryData', () => {
  // ── Null / empty / falsy guards ───────────────────────────────────────

  describe('null and empty input handling', () => {
    it('should return false for null input', () => {
      // GIVEN: A null value cast as ArrayBuffer
      const data = null as unknown as ArrayBuffer;

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Rejects it
      expect(result).toBe(false);
    });

    it('should return false for undefined input', () => {
      // GIVEN: An undefined value cast as ArrayBuffer
      const data = undefined as unknown as ArrayBuffer;

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Rejects it
      expect(result).toBe(false);
    });

    it('should return false for a zero-length ArrayBuffer', () => {
      // GIVEN: An empty ArrayBuffer
      const data = new ArrayBuffer(0);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Rejects it -- no version byte present
      expect(result).toBe(false);
    });
  });

  // ── Size limits ───────────────────────────────────────────────────────

  describe('maximum size enforcement (50 MB)', () => {
    const MAX_SIZE = 50 * 1024 * 1024; // 50 MB

    it('should return false for buffer exceeding 50 MB', () => {
      // GIVEN: A buffer 1 byte over the 50 MB limit with a valid version
      const data = createBufferWithVersion(3, MAX_SIZE + 1);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Rejects it
      expect(result).toBe(false);
    });

    it('should return true for buffer exactly at 50 MB with valid version', () => {
      // GIVEN: A buffer exactly 50 MB with version 3
      const data = createBufferWithVersion(3, MAX_SIZE);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Accepts it -- boundary is inclusive
      expect(result).toBe(true);
    });

    it('should return true for buffer 1 byte under 50 MB with valid version', () => {
      // GIVEN: A buffer 1 byte below the limit with version 3
      const data = createBufferWithVersion(3, MAX_SIZE - 1);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Accepts it
      expect(result).toBe(true);
    });
  });

  // ── Version byte validation ───────────────────────────────────────────

  describe('valid protocol versions [3, 4, 5]', () => {
    it('should return true for version byte 3 (V3)', () => {
      // GIVEN: A 16-byte buffer with version byte set to 3
      const data = createBufferWithVersion(3, 16);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Accepts V3
      expect(result).toBe(true);
    });

    it('should return true for version byte 4 (V4)', () => {
      // GIVEN: A buffer with version byte set to 4
      const data = createBufferWithVersion(4, 16);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Accepts V4
      expect(result).toBe(true);
    });

    it('should return true for version byte 5 (V5)', () => {
      // GIVEN: A buffer with version byte set to 5
      const data = createBufferWithVersion(5, 16);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Accepts V5
      expect(result).toBe(true);
    });
  });

  describe('rejected protocol versions', () => {
    it('should return false for version byte 2 (V2 removed)', () => {
      // GIVEN: A buffer with version byte 2 -- V2 was explicitly removed
      const data = createBufferWithVersion(2, 16);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Rejects V2
      expect(result).toBe(false);
    });

    it('should return false for version byte 1 (unsupported legacy)', () => {
      // GIVEN: A buffer with version byte 1
      const data = createBufferWithVersion(1, 16);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Rejects V1
      expect(result).toBe(false);
    });

    it('should return false for version byte 0 (invalid)', () => {
      // GIVEN: A buffer with version byte 0
      const data = createBufferWithVersion(0, 16);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Rejects version 0
      expect(result).toBe(false);
    });

    it('should return false for version byte 6 (future/unsupported)', () => {
      // GIVEN: A buffer with version byte 6
      const data = createBufferWithVersion(6, 16);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Rejects version 6
      expect(result).toBe(false);
    });

    it('should return false for version byte 255 (max uint8)', () => {
      // GIVEN: A buffer with version byte at maximum uint8 value
      const data = createBufferWithVersion(255, 16);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Rejects version 255
      expect(result).toBe(false);
    });

    it('should return false for version byte 128 (arbitrary high value)', () => {
      // GIVEN: A buffer with version byte 128
      const data = createBufferWithVersion(128, 16);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Rejects version 128
      expect(result).toBe(false);
    });
  });

  // ── Minimum viable buffer ─────────────────────────────────────────────

  describe('minimum viable buffer (1 byte)', () => {
    it('should accept a 1-byte buffer with valid version 3', () => {
      // GIVEN: The smallest possible valid buffer -- just the version byte
      const data = createBufferWithVersion(3, 1);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Accepts it -- validation only checks version, not payload
      expect(result).toBe(true);
    });

    it('should reject a 1-byte buffer with invalid version 0', () => {
      // GIVEN: A single-byte buffer with invalid version
      const data = createBufferWithVersion(0, 1);

      // WHEN: Validating
      const result = validateBinaryData(data);

      // THEN: Rejects it
      expect(result).toBe(false);
    });
  });

  // ── Exhaustive version sweep ──────────────────────────────────────────

  describe('exhaustive version byte sweep (0-10)', () => {
    const expectedResults: Array<[number, boolean]> = [
      [0, false],
      [1, false],
      [2, false], // V2 explicitly removed
      [3, true],  // V3 valid
      [4, true],  // V4 valid
      [5, true],  // V5 valid
      [6, false],
      [7, false],
      [8, false],
      [9, false],
      [10, false],
    ];

    it.each(expectedResults)(
      'version byte %i should return %s',
      (version, expected) => {
        // GIVEN: A buffer with the specified version byte
        const data = createBufferWithVersion(version, 16);

        // WHEN: Validating
        const result = validateBinaryData(data);

        // THEN: Matches expected acceptance/rejection
        expect(result).toBe(expected);
      },
    );
  });
});

describe('resetBinaryState', () => {
  it('should complete without throwing', () => {
    // GIVEN: Module is in its initial state
    // WHEN: Resetting binary state
    // THEN: No exception is thrown
    expect(() => resetBinaryState()).not.toThrow();
  });

  it('should be idempotent -- calling twice does not throw', () => {
    // GIVEN: State has already been reset once
    resetBinaryState();

    // WHEN: Resetting again
    // THEN: Still no exception
    expect(() => resetBinaryState()).not.toThrow();
  });
});

describe('VALID_VERSIONS constant (inline in validateBinaryData)', () => {
  it('should not accept version 2 -- V2 was removed from VALID_VERSIONS', () => {
    // GIVEN: The change that removed V2 from the valid set [3, 4, 5]
    const v2Buffer = createBufferWithVersion(2, 16);

    // WHEN: Validating a V2 buffer
    const result = validateBinaryData(v2Buffer);

    // THEN: V2 is definitively rejected
    expect(result).toBe(false);
  });

  it('should accept exactly versions 3, 4, and 5', () => {
    // GIVEN: All three currently valid versions
    const validVersions = [3, 4, 5];

    for (const version of validVersions) {
      // WHEN: Validating each
      const result = validateBinaryData(createBufferWithVersion(version, 16));

      // THEN: All accepted
      expect(result).toBe(true);
    }
  });

  it('should reject all versions outside [3, 4, 5] in the 0-10 range', () => {
    // GIVEN: Every version byte value from 0 to 10 that is NOT in [3, 4, 5]
    const invalidVersions = [0, 1, 2, 6, 7, 8, 9, 10];

    for (const version of invalidVersions) {
      // WHEN: Validating each
      const result = validateBinaryData(createBufferWithVersion(version, 16));

      // THEN: All rejected
      expect(result).toBe(false);
    }
  });
});
