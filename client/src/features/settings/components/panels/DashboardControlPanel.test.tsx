import { describe, it, expect, beforeEach, vi } from 'vitest';
import React from 'react';
import { render } from '@testing-library/react';

vi.mock('../../../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

vi.mock('lucide-react', () => ({
  Activity: () => React.createElement('span', null, 'Activity'),
  RefreshCw: () => React.createElement('span', null, 'RefreshCw'),
  Gauge: () => React.createElement('span', null, 'Gauge'),
  Layers: () => React.createElement('span', null, 'Layers'),
  TrendingUp: () => React.createElement('span', null, 'TrendingUp'),
  Settings: () => React.createElement('span', null, 'Settings'),
  Cpu: () => React.createElement('span', null, 'Cpu'),
  Monitor: () => React.createElement('span', null, 'Monitor'),
}));

vi.mock('../../../../rendering/rendererFactory', () => ({
  rendererCapabilities: { webgpu: false, webgl2: true },
  isWebGPURenderer: () => false,
}));

vi.mock('../../../../store/settingsStore', () => ({
  useSettingsStore: (selector: any) => {
    const state = {
      settings: {
        dashboard: {
          autoRefresh: false,
          refreshInterval: 10000,
          computeMode: 'basic-force-directed',
        },
      },
      updateSettings: vi.fn(),
    };
    return selector ? selector(state) : state;
  },
}));

// Mock all design system components
const createMockComponent = (name: string) => (props: any) =>
  React.createElement('div', { 'data-testid': name, ...props }, props.children);

vi.mock('@/features/design-system/components/Button', () => ({
  Button: createMockComponent('button'),
}));
vi.mock('@/features/design-system/components/Card', () => ({
  Card: createMockComponent('card'),
  CardContent: createMockComponent('card-content'),
  CardDescription: createMockComponent('card-desc'),
  CardHeader: createMockComponent('card-header'),
  CardTitle: createMockComponent('card-title'),
}));
vi.mock('@/features/design-system/components/Switch', () => ({
  Switch: createMockComponent('switch'),
}));
vi.mock('@/features/design-system/components/Select', () => ({
  Select: createMockComponent('select'),
  SelectContent: createMockComponent('select-content'),
  SelectItem: createMockComponent('select-item'),
  SelectTrigger: createMockComponent('select-trigger'),
  SelectValue: createMockComponent('select-value'),
}));
vi.mock('@/features/design-system/components/Slider', () => ({
  Slider: createMockComponent('slider'),
}));
vi.mock('@/features/design-system/components/Badge', () => ({
  Badge: createMockComponent('badge'),
}));
vi.mock('@/features/design-system/components/Separator', () => ({
  Separator: createMockComponent('separator'),
}));
vi.mock('@/features/design-system/components/Label', () => ({
  Label: createMockComponent('label'),
}));

import { DashboardControlPanel } from './DashboardControlPanel';

describe('DashboardControlPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false }));
  });

  it('renders without crashing', () => {
    const { container } = render(React.createElement(DashboardControlPanel));
    expect(container).toBeDefined();
  });

  it('renders with settings from store', () => {
    const { container } = render(React.createElement(DashboardControlPanel));
    expect(container.innerHTML.length).toBeGreaterThan(0);
  });

  it('does not poll when autoRefresh is disabled', () => {
    render(React.createElement(DashboardControlPanel));
    expect(fetch).not.toHaveBeenCalled();
  });
});
