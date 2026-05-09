import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';

// --- Mock all external dependencies before importing the hook ---

vi.mock('../../../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

const mockInitPod = vi.fn();
const mockCheckPodExists = vi.fn();
const mockDeleteResource = vi.fn();

vi.mock('../../../../services/SolidPodService', () => ({
  default: {
    initPod: (...args: unknown[]) => mockInitPod(...args),
    checkPodExists: (...args: unknown[]) => mockCheckPodExists(...args),
    deleteResource: (...args: unknown[]) => mockDeleteResource(...args),
  },
}));

const mockIsAuthenticated = vi.fn(() => false);
vi.mock('../../../../services/nostrAuthService', () => ({
  nostrAuth: {
    isAuthenticated: () => mockIsAuthenticated(),
    getCurrentUser: vi.fn(() => null),
    initialized: true,
    initialize: vi.fn(),
  },
}));

const mockUseNostrAuth = vi.fn(() => ({ authenticated: false }));
vi.mock('../../../../hooks/useNostrAuth', () => ({
  useNostrAuth: () => mockUseNostrAuth(),
}));

import { useSolidPod } from '../useSolidPod';

describe('useSolidPod', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseNostrAuth.mockReturnValue({ authenticated: false });
    mockIsAuthenticated.mockReturnValue(false);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  // ---- Initial state ----

  describe('initial state', () => {
    it('should start with null podInfo and no error', () => {
      const { result } = renderHook(() => useSolidPod());
      expect(result.current.podInfo).toBeNull();
      expect(result.current.error).toBeNull();
      expect(result.current.isLoading).toBe(false);
    });

    it('should not auto-check pod when not authenticated', () => {
      renderHook(() => useSolidPod());
      expect(mockInitPod).not.toHaveBeenCalled();
    });
  });

  // ---- Auto-check on authentication ----

  describe('auto-check on authentication', () => {
    it('should auto-check pod when authenticated', async () => {
      mockUseNostrAuth.mockReturnValue({ authenticated: true });
      mockIsAuthenticated.mockReturnValue(true);
      mockInitPod.mockResolvedValueOnce({
        success: true,
        podUrl: 'http://localhost:8484/pods/user1/',
        webId: 'http://localhost:8484/pods/user1/profile/card#me',
        structure: { containers: ['/public/', '/private/'] },
      });

      const { result } = renderHook(() => useSolidPod());

      await waitFor(() => {
        expect(result.current.podInfo).not.toBeNull();
      });

      expect(result.current.podInfo!.exists).toBe(true);
    });

    it('should fall back to checkPodExists when initPod fails', async () => {
      mockUseNostrAuth.mockReturnValue({ authenticated: true });
      mockIsAuthenticated.mockReturnValue(true);
      mockInitPod.mockResolvedValueOnce({ success: false });
      mockCheckPodExists.mockResolvedValueOnce({
        exists: false,
        podUrl: undefined,
        webId: undefined,
      });

      const { result } = renderHook(() => useSolidPod());

      await waitFor(() => {
        expect(mockCheckPodExists).toHaveBeenCalled();
      });

      expect(result.current.podInfo!.exists).toBe(false);
    });
  });

  // ---- checkPod ----

  describe('checkPod', () => {
    it('should set isLoading while checking', async () => {
      let resolveInit!: (value: any) => void;
      mockInitPod.mockReturnValueOnce(new Promise((resolve) => { resolveInit = resolve; }));

      const { result } = renderHook(() => useSolidPod());

      act(() => {
        result.current.checkPod();
      });

      // isLoading is true while waiting
      expect(result.current.isLoading).toBe(true);

      await act(async () => {
        resolveInit({ success: true, podUrl: '/solid/pod/', webId: '/solid/pod/card#me' });
      });

      expect(result.current.isLoading).toBe(false);
    });

    it('should set error on exception', async () => {
      mockInitPod.mockRejectedValueOnce(new Error('Connection refused'));

      const { result } = renderHook(() => useSolidPod());

      await act(async () => {
        await result.current.checkPod();
      });

      expect(result.current.error).toBe('Connection refused');
      expect(result.current.isLoading).toBe(false);
    });

    it('should rewrite JSS internal URLs to public proxy paths', async () => {
      mockUseNostrAuth.mockReturnValue({ authenticated: true });
      mockIsAuthenticated.mockReturnValue(true);
      mockInitPod.mockResolvedValueOnce({
        success: true,
        podUrl: 'http://visionflow-jss:8484/pods/user1/',
        webId: 'http://visionflow-jss:8484/pods/user1/profile/card#me',
        structure: {},
      });

      const { result } = renderHook(() => useSolidPod());

      await waitFor(() => {
        expect(result.current.podInfo).not.toBeNull();
      });

      expect(result.current.podInfo!.podUrl).toBe('/solid/pods/user1/');
      expect(result.current.podInfo!.webId).toBe('/solid/pods/user1/profile/card#me');
    });
  });

  // ---- createPod ----

  describe('createPod', () => {
    it('should return success result when initPod succeeds', async () => {
      mockInitPod.mockResolvedValueOnce({
        success: true,
        podUrl: 'http://localhost:8484/pods/new/',
        webId: 'http://localhost:8484/pods/new/card#me',
        created: true,
        structure: {},
      });

      const { result } = renderHook(() => useSolidPod());
      let createResult: any;

      await act(async () => {
        createResult = await result.current.createPod('testpod');
      });

      expect(createResult.success).toBe(true);
      expect(result.current.podInfo!.exists).toBe(true);
    });

    it('should return error result when initPod fails', async () => {
      mockInitPod.mockResolvedValueOnce({
        success: false,
        error: 'Pod limit reached',
      });

      const { result } = renderHook(() => useSolidPod());
      let createResult: any;

      await act(async () => {
        createResult = await result.current.createPod();
      });

      expect(createResult.success).toBe(false);
      expect(createResult.error).toBe('Pod limit reached');
      expect(result.current.error).toBe('Pod limit reached');
    });

    it('should handle exceptions during creation', async () => {
      mockInitPod.mockRejectedValueOnce(new Error('Timeout'));

      const { result } = renderHook(() => useSolidPod());
      let createResult: any;

      await act(async () => {
        createResult = await result.current.createPod();
      });

      expect(createResult.success).toBe(false);
      expect(createResult.error).toBe('Timeout');
    });
  });

  // ---- deletePod ----

  describe('deletePod', () => {
    it('should return false when no podInfo exists', async () => {
      const { result } = renderHook(() => useSolidPod());

      let deleteResult: boolean;
      await act(async () => {
        deleteResult = await result.current.deletePod();
      });

      expect(deleteResult!).toBe(false);
    });

    it('should call deleteResource and clear podInfo on success', async () => {
      mockDeleteResource.mockResolvedValueOnce(true);

      const { result } = renderHook(() => useSolidPod());

      // Manually set podInfo to simulate an existing pod
      await act(async () => {
        mockInitPod.mockResolvedValueOnce({
          success: true,
          podUrl: 'http://localhost:8484/pods/test/',
          webId: 'http://localhost:8484/pods/test/card#me',
        });
        await result.current.checkPod();
      });

      expect(result.current.podInfo!.exists).toBe(true);

      await act(async () => {
        const deleted = await result.current.deletePod();
        expect(deleted).toBe(true);
      });

      expect(result.current.podInfo!.exists).toBe(false);
    });

    it('should set error on delete failure', async () => {
      mockDeleteResource.mockRejectedValueOnce(new Error('Forbidden'));

      const { result } = renderHook(() => useSolidPod());

      // Set up existing pod
      await act(async () => {
        mockInitPod.mockResolvedValueOnce({
          success: true,
          podUrl: '/solid/pods/test/',
          webId: '/solid/pods/test/card#me',
        });
        await result.current.checkPod();
      });

      await act(async () => {
        const deleted = await result.current.deletePod();
        expect(deleted).toBe(false);
      });

      expect(result.current.error).toBe('Forbidden');
    });
  });
});
