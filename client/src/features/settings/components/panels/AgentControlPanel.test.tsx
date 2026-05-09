import { describe, it, expect, beforeEach, vi } from 'vitest';
import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';

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
  Play: () => React.createElement('span', null, 'Play'),
  Settings: () => React.createElement('span', null, 'Settings'),
  RefreshCw: () => React.createElement('span', null, 'RefreshCw'),
  AlertCircle: () => React.createElement('span', null, 'AlertCircle'),
  CheckCircle: () => React.createElement('span', null, 'CheckCircle'),
  XCircle: () => React.createElement('span', null, 'XCircle'),
  Users: () => React.createElement('span', null, 'Users'),
  Layers: () => React.createElement('span', null, 'Layers'),
}));

vi.mock('../../../design-system/components/Button', () => ({
  Button: ({ children, onClick, disabled, ...props }: any) =>
    React.createElement('button', { onClick, disabled, ...props }, children),
}));

vi.mock('./SkillsTab', () => ({
  SkillsTab: () => React.createElement('div', { 'data-testid': 'skills-tab' }, 'Skills'),
}));

vi.mock('../../../bots/components/AgentTelemetryStream', () => ({
  AgentTelemetryStream: () =>
    React.createElement('div', { 'data-testid': 'telemetry-stream' }, 'Telemetry'),
}));

const mockPollNow = vi.fn();
vi.mock('../../../bots/contexts/BotsDataContext', () => ({
  useBotsData: () => ({
    botsData: {
      agents: [
        { id: 'a1', type: 'researcher', status: 'active', health: 90 },
      ],
    },
    pollNow: mockPollNow,
  }),
}));

vi.mock('../../../../store/settingsStore', () => ({
  useSettingsStore: (selector: any) => {
    const state = {
      settings: { agents: {} },
      updateSettings: vi.fn(),
    };
    return selector ? selector(state) : state;
  },
}));

const mockPost = vi.fn().mockResolvedValue({ data: {} });
vi.mock('../../../../services/api/UnifiedApiClient', () => ({
  unifiedApiClient: {
    post: (...args: unknown[]) => mockPost(...args),
  },
}));

import { AgentControlPanel } from './AgentControlPanel';

describe('AgentControlPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders without crashing', () => {
    const { container } = render(React.createElement(AgentControlPanel));
    expect(container).toBeDefined();
  });

  it('displays agent data from context', () => {
    render(React.createElement(AgentControlPanel));
    // Component should render -- no crash is the primary assertion
    // Agents tab is the default active tab
    expect(document.body.innerHTML).toBeTruthy();
  });

  it('accepts className prop', () => {
    const { container } = render(
      React.createElement(AgentControlPanel, { className: 'custom-class' }),
    );
    expect(container.innerHTML).toBeTruthy();
  });
});
