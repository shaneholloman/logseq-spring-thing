import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import React from 'react';

// --- Mock all external dependencies ---

vi.mock('../../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

const mockUseFeatureFlag = vi.fn(() => true);
vi.mock('../../../services/featureFlags', () => ({
  useFeatureFlag: (flag: string) => mockUseFeatureFlag(flag),
}));

const mockOverlay: Record<string, any> = {};
const mockBusyId: string | null = null;
const mockError: string | null = null;
const mockPublish = vi.fn();
const mockUnpublish = vi.fn();
const mockClearError = vi.fn();

vi.mock('../../graph/store/visibilitySlice', () => ({
  useVisibilityStore: (selector: (s: any) => any) => {
    const state = {
      overlay: mockOverlay,
      busyId: mockBusyId,
      error: mockError,
      publish: mockPublish,
      unpublish: mockUnpublish,
      clearError: mockClearError,
    };
    return selector(state);
  },
}));

vi.mock('../../../services/nostrAuthService', () => ({
  nostrAuth: {
    getCurrentUser: vi.fn(() => ({ pubkey: 'abcd1234' })),
    hexToNpub: vi.fn((hex: string) => `npub1${hex.slice(0, 8)}`),
    isAuthenticated: vi.fn(() => true),
    initialized: true,
    initialize: vi.fn(),
  },
}));

// Mock design-system components to minimal HTML
vi.mock('../../design-system/components', () => ({
  Badge: ({ children, variant, className, title, ...props }: any) => (
    <span data-variant={variant} className={className} title={title} {...props}>{children}</span>
  ),
  Button: ({ children, onClick, disabled, variant, size, loading, ...props }: any) => (
    <button onClick={onClick} disabled={disabled} data-variant={variant} {...props}>{children}</button>
  ),
  Dialog: ({ children, open, onOpenChange }: any) => (
    open ? <div data-testid="dialog">{children}</div> : null
  ),
  DialogContent: ({ children }: any) => <div>{children}</div>,
  DialogDescription: ({ children }: any) => <p>{children}</p>,
  DialogFooter: ({ children }: any) => <div>{children}</div>,
  DialogHeader: ({ children }: any) => <div>{children}</div>,
  DialogTitle: ({ children }: any) => <h2>{children}</h2>,
}));

import { VisibilityControl } from '../VisibilityControl';
import type { KGNode } from '../../graph/types/graphTypes';

function makeNode(overrides: Partial<KGNode> = {}): KGNode {
  return {
    id: 'node-1',
    label: 'Test Node',
    visibility: 'public',
    owner_pubkey: undefined,
    pod_url: undefined,
    ...overrides,
  } as KGNode;
}

describe('VisibilityControl', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseFeatureFlag.mockReturnValue(true);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  // ---- Feature flag disabled ----

  describe('feature flag disabled', () => {
    it('should render a plain Public badge when VISIBILITY_TRANSITIONS is off', () => {
      mockUseFeatureFlag.mockReturnValue(false);
      render(<VisibilityControl node={makeNode()} />);
      expect(screen.getByText('Public')).toBeTruthy();
    });

    it('should not render any toggle button when flag is off', () => {
      mockUseFeatureFlag.mockReturnValue(false);
      render(<VisibilityControl node={makeNode()} />);
      expect(screen.queryByRole('button')).toBeNull();
    });
  });

  // ---- Non-owner view ----

  describe('non-owner view', () => {
    it('should show Public badge for public nodes', () => {
      render(<VisibilityControl node={makeNode()} forceOwner={false} />);
      expect(screen.getByText('Public')).toBeTruthy();
      expect(screen.queryByRole('button')).toBeNull();
    });

    it('should show Private badge with truncated npub for private nodes', () => {
      const node = makeNode({ visibility: 'private', owner_pubkey: 'deadbeef0123456789abcdef' });
      render(<VisibilityControl node={node} forceOwner={false} />);
      const badge = screen.getByText(/Private/);
      expect(badge).toBeTruthy();
      expect(badge.textContent).toContain('owner:');
    });
  });

  // ---- Owner view ----

  describe('owner view', () => {
    it('should render toggle button for owner of public node', () => {
      render(<VisibilityControl node={makeNode()} forceOwner={true} />);
      const btn = screen.getByRole('button', { name: /make private/i });
      expect(btn).toBeTruthy();
    });

    it('should render Publish button for owner of private node', () => {
      const node = makeNode({ visibility: 'private' });
      render(<VisibilityControl node={node} forceOwner={true} />);
      const btn = screen.getByRole('button', { name: /publish/i });
      expect(btn).toBeTruthy();
    });

    it('should show confirmation dialog when toggle is clicked', () => {
      render(<VisibilityControl node={makeNode()} forceOwner={true} />);
      fireEvent.click(screen.getByRole('button', { name: /make private/i }));
      expect(screen.getByTestId('dialog')).toBeTruthy();
      expect(screen.getByText(/Unpublish node/)).toBeTruthy();
    });

    it('should show pod link when public node has pod_url', () => {
      const node = makeNode({ pod_url: 'https://pod.example.com/resource/1' });
      render(<VisibilityControl node={node} forceOwner={true} />);
      const link = screen.getByText('pod');
      expect(link).toBeTruthy();
      expect(link.getAttribute('href')).toBe('https://pod.example.com/resource/1');
      expect(link.getAttribute('target')).toBe('_blank');
    });
  });

  // ---- Tombstone state ----

  describe('tombstone state', () => {
    it('should render Tombstoned badge for tombstone visibility', () => {
      const node = makeNode({ visibility: 'tombstone' as any });
      render(<VisibilityControl node={node} forceOwner={true} />);
      expect(screen.getByText('Tombstoned')).toBeTruthy();
      // No toggle button for tombstoned nodes
      expect(screen.queryByRole('button')).toBeNull();
    });
  });

  // ---- className prop ----

  describe('className prop', () => {
    it('should pass className through to rendered output', () => {
      mockUseFeatureFlag.mockReturnValue(false);
      const { container } = render(
        <VisibilityControl node={makeNode()} className="custom-class" />,
      );
      expect(container.querySelector('.custom-class')).toBeTruthy();
    });
  });
});
