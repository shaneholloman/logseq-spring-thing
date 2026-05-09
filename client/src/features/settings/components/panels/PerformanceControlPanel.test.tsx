import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
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
  Zap: () => React.createElement('span', null, 'Zap'),
  Cpu: () => React.createElement('span', null, 'Cpu'),
  MemoryStick: () => React.createElement('span', null, 'MemoryStick'),
  Gauge: () => React.createElement('span', null, 'Gauge'),
  TrendingUp: () => React.createElement('span', null, 'TrendingUp'),
  Activity: () => React.createElement('span', null, 'Activity'),
  Settings: () => React.createElement('span', null, 'Settings'),
}));

vi.mock('@/store/settingsStore', () => ({
  useSettingsStore: (selector: any) => {
    const state = {
      settings: {
        performance: {
          showFPS: true,
          targetFPS: 60,
          levelOfDetail: 'medium',
          enableAdaptiveQuality: true,
        },
      },
      updateSettings: vi.fn(),
    };
    return selector ? selector(state) : state;
  },
}));

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
vi.mock('@/features/design-system/components/Alert', () => ({
  Alert: createMockComponent('alert'),
  AlertDescription: createMockComponent('alert-desc'),
}));

import { PerformanceControlPanel } from './PerformanceControlPanel';

describe('PerformanceControlPanel', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false }));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders without crashing', () => {
    const { container } = render(React.createElement(PerformanceControlPanel));
    expect(container).toBeDefined();
  });

  it('renders performance controls from settings', () => {
    const { container } = render(React.createElement(PerformanceControlPanel));
    expect(container.innerHTML.length).toBeGreaterThan(0);
  });

  it('polls metrics after initial delay', async () => {
    render(React.createElement(PerformanceControlPanel));

    await vi.advanceTimersByTimeAsync(3500);

    expect(fetch).toHaveBeenCalledWith('/api/performance/metrics');
  });
});
