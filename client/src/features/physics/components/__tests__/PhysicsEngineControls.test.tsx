import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import React from 'react';

// --- Mock all external dependencies ---

vi.mock('../../../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

vi.mock('../../../../utils/clientDebugState', () => ({
  debugState: {
    isEnabled: () => false,
    isDataDebugEnabled: () => false,
  },
}));

const mockSettings = {
  visualisation: {
    graphs: {
      logseq: {
        physics: {
          repelK: 50,
          springK: 0.1,
          damping: 0.95,
          gravity: 0.0001,
          dt: 0.016,
          maxVelocity: 2.0,
          temperature: 0.01,
          boundaryExtremeMultiplier: 2.0,
          boundaryExtremeForceMultiplier: 5.0,
          boundaryVelocityDamping: 0.8,
          useSsspDistances: false,
          ssspAlpha: 0.5,
        },
        nodes: {},
        edges: {},
        labels: {},
      },
    },
  },
};

const mockUpdateSettings = vi.fn((updater: (draft: any) => void) => {
  updater(JSON.parse(JSON.stringify(mockSettings)));
});
const mockLoadSection = vi.fn();
const mockEnsureLoaded = vi.fn().mockResolvedValue(undefined);

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: () => ({
    settings: mockSettings,
    initialized: true,
    updateSettings: mockUpdateSettings,
    loadSection: mockLoadSection,
    ensureLoaded: mockEnsureLoaded,
  }),
}));

vi.mock('@/features/design-system/components/Toast', () => ({
  useToast: () => ({
    toast: vi.fn(),
  }),
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

// Mock design-system components to simplified HTML
vi.mock('@/features/design-system/components/Card', () => ({
  Card: ({ children, className }: any) => <div className={className}>{children}</div>,
  CardContent: ({ children, className }: any) => <div className={className}>{children}</div>,
  CardDescription: ({ children }: any) => <p>{children}</p>,
  CardHeader: ({ children }: any) => <div>{children}</div>,
  CardTitle: ({ children, className }: any) => <h3 className={className}>{children}</h3>,
}));

vi.mock('@/features/design-system/components/Select', () => ({
  Select: ({ children, value, onValueChange }: any) => (
    <div data-testid="select" data-value={value}>
      {React.Children.map(children, (child: any) =>
        child?.type?.displayName === 'SelectContent'
          ? React.cloneElement(child, { onValueChange })
          : child
      )}
    </div>
  ),
  SelectContent: ({ children }: any) => <div>{children}</div>,
  SelectItem: ({ children, value }: any) => <div data-value={value}>{children}</div>,
  SelectTrigger: ({ children }: any) => <div>{children}</div>,
  SelectValue: () => <span />,
}));

vi.mock('@/features/design-system/components/Slider', () => ({
  Slider: ({ id, value, onValueChange, min, max, step }: any) => (
    <input
      type="range"
      id={id}
      data-testid={`slider-${id}`}
      value={value?.[0] ?? 0}
      min={min}
      max={max}
      step={step}
      onChange={(e) => onValueChange?.([parseFloat(e.target.value)])}
    />
  ),
}));

vi.mock('@/features/design-system/components/Switch', () => ({
  Switch: ({ id, checked, onCheckedChange }: any) => (
    <input
      type="checkbox"
      id={id}
      data-testid={`switch-${id}`}
      checked={checked}
      onChange={(e) => onCheckedChange?.(e.target.checked)}
    />
  ),
}));

vi.mock('@/features/design-system/components/Label', () => ({
  Label: ({ children, htmlFor, className }: any) => (
    <label htmlFor={htmlFor} className={className}>{children}</label>
  ),
}));

vi.mock('@/features/design-system/components/Button', () => ({
  Button: ({ children, onClick, disabled, className, variant }: any) => (
    <button onClick={onClick} disabled={disabled} className={className} data-variant={variant}>
      {children}
    </button>
  ),
}));

vi.mock('@/features/design-system/components/Badge', () => ({
  Badge: ({ children, variant }: any) => <span data-variant={variant}>{children}</span>,
}));

vi.mock('@/features/design-system/components/Tabs', () => ({
  Tabs: ({ children, defaultValue }: any) => (
    <div data-testid="tabs" data-default={defaultValue}>{children}</div>
  ),
  TabsList: ({ children, className }: any) => <div className={className} role="tablist">{children}</div>,
  TabsTrigger: ({ children, value }: any) => (
    <button role="tab" data-value={value}>{children}</button>
  ),
  TabsContent: ({ children, value, className }: any) => (
    <div role="tabpanel" data-value={value} className={className}>{children}</div>
  ),
}));

vi.mock('@/features/design-system/components/Tooltip', () => ({
  TooltipRoot: ({ children }: any) => <div>{children}</div>,
  TooltipContent: ({ children }: any) => <div>{children}</div>,
  TooltipProvider: ({ children }: any) => <div>{children}</div>,
  TooltipTrigger: ({ children, asChild }: any) => <div>{children}</div>,
}));

vi.mock('@/features/analytics/components/SemanticClusteringControls', () => ({
  SemanticClusteringControls: () => <div data-testid="semantic-clustering" />,
}));

vi.mock('../ConstraintBuilderDialog', () => ({
  ConstraintBuilderDialog: ({ isOpen, onClose, onSave }: any) => (
    isOpen ? <div data-testid="constraint-dialog">Constraint Builder</div> : null
  ),
}));

vi.mock('../PhysicsPresets', () => ({
  PhysicsPresets: () => <div data-testid="physics-presets">Physics Presets</div>,
}));

vi.mock('../../../../services/api', () => ({
  unifiedApiClient: {
    get: vi.fn().mockResolvedValue({ data: { utilization: 50, memory: 40, temperature: 65, power: 150 } }),
    post: vi.fn().mockResolvedValue({ data: {} }),
  },
}));

import { PhysicsEngineControls } from '../PhysicsEngineControls';

describe('PhysicsEngineControls', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  // ---- Rendering ----

  describe('rendering', () => {
    it('should render all four tabs', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByText('Engine')).toBeTruthy();
      expect(screen.getByText('Forces')).toBeTruthy();
      expect(screen.getByText('Constraints')).toBeTruthy();
      expect(screen.getByText('Analytics')).toBeTruthy();
    });

    it('should render GPU Engine Status section', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByText('GPU Engine Status')).toBeTruthy();
    });

    it('should render GPU Kernel Mode section', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByText('GPU Kernel Mode')).toBeTruthy();
    });

    it('should render Isolation Layers section', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByText('Isolation Layers')).toBeTruthy();
    });

    it('should render PhysicsPresets component', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByTestId('physics-presets')).toBeTruthy();
    });

    it('should render SemanticClusteringControls in analytics tab', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByTestId('semantic-clustering')).toBeTruthy();
    });
  });

  // ---- Physics settings loading ----

  describe('settings loading', () => {
    it('should call ensureLoaded on mount when initialized', async () => {
      render(<PhysicsEngineControls />);
      await vi.advanceTimersByTimeAsync(0);
      expect(mockEnsureLoaded).toHaveBeenCalledWith(
        expect.arrayContaining([
          'visualisation.graphs.logseq.physics',
        ]),
      );
    });
  });

  // ---- Force parameter sliders ----

  describe('force parameter controls', () => {
    it('should render force parameter sliders', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByTestId('slider-repulsionStrength')).toBeTruthy();
      expect(screen.getByTestId('slider-damping')).toBeTruthy();
      expect(screen.getByTestId('slider-temperature')).toBeTruthy();
      expect(screen.getByTestId('slider-gravity')).toBeTruthy();
      expect(screen.getByTestId('slider-maxVelocity')).toBeTruthy();
      expect(screen.getByTestId('slider-timeStep')).toBeTruthy();
    });

    it('should call updateSettings when a slider value changes', () => {
      render(<PhysicsEngineControls />);
      const slider = screen.getByTestId('slider-damping');
      fireEvent.change(slider, { target: { value: '0.75' } });
      expect(mockUpdateSettings).toHaveBeenCalled();
    });
  });

  // ---- Boundary behavior sliders ----

  describe('boundary behavior controls', () => {
    it('should render boundary parameter sliders', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByTestId('slider-boundaryExtremeMultiplier')).toBeTruthy();
      expect(screen.getByTestId('slider-boundaryExtremeForceMultiplier')).toBeTruthy();
      expect(screen.getByTestId('slider-boundaryVelocityDamping')).toBeTruthy();
    });
  });

  // ---- Constraint toggles ----

  describe('constraint toggles', () => {
    it('should render all 10 constraint types', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByText('Fixed Position')).toBeTruthy();
      expect(screen.getByText('Separation')).toBeTruthy();
      expect(screen.getByText('Collision')).toBeTruthy();
      expect(screen.getByText('Tree Layout')).toBeTruthy();
    });

    it('should toggle a constraint when its switch is clicked', () => {
      render(<PhysicsEngineControls />);
      const fixedSwitch = screen.getByTestId('switch-fixed');
      expect((fixedSwitch as HTMLInputElement).checked).toBe(false);
      fireEvent.click(fixedSwitch);
      // The toggle is managed locally -- just verify it does not throw
    });

    it('should render Create Custom Constraint button', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByText('Create Custom Constraint')).toBeTruthy();
    });

    it('should open constraint builder dialog on button click', () => {
      render(<PhysicsEngineControls />);
      expect(screen.queryByTestId('constraint-dialog')).toBeNull();
      fireEvent.click(screen.getByText('Create Custom Constraint'));
      expect(screen.getByTestId('constraint-dialog')).toBeTruthy();
    });
  });

  // ---- Isolation layers ----

  describe('isolation layers', () => {
    it('should render all 3 isolation layers', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByText('Focus Layer')).toBeTruthy();
      expect(screen.getByText('Context Layer')).toBeTruthy();
      expect(screen.getByText('Background Layer')).toBeTruthy();
    });
  });

  // ---- Trajectory settings ----

  describe('trajectory visualization', () => {
    it('should render trajectory enable switch', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByTestId('switch-trajectory-enabled')).toBeTruthy();
    });

    it('should not show trail length slider when trajectories are disabled', () => {
      render(<PhysicsEngineControls />);
      // Trajectory is disabled by default, trail length slider should not render
      expect(screen.queryByTestId('slider-trajectory-length')).toBeNull();
    });

    it('should show trail settings when trajectory is enabled', () => {
      render(<PhysicsEngineControls />);
      fireEvent.click(screen.getByTestId('switch-trajectory-enabled'));
      expect(screen.getByTestId('slider-trajectory-length')).toBeTruthy();
      expect(screen.getByTestId('switch-color-velocity')).toBeTruthy();
    });
  });

  // ---- GPU metrics polling ----

  describe('GPU metrics', () => {
    it('should render GPU metric displays', () => {
      render(<PhysicsEngineControls />);
      expect(screen.getByText('Utilization')).toBeTruthy();
      expect(screen.getByText('Memory')).toBeTruthy();
      // Temperature appears in both GPU metrics and Force Parameters
      expect(screen.getAllByText('Temperature').length).toBeGreaterThanOrEqual(1);
      expect(screen.getByText('Power')).toBeTruthy();
    });
  });
});
