

export interface ControlPanelProps {
  showStats: boolean;
  enableBloom: boolean;
  onOrbitControlsToggle?: (enabled: boolean) => void;
  botsData?: BotsData;
  graphData?: GraphData;
  otherGraphData?: GraphData;
}

export interface BotsData {
  nodeCount: number;
  edgeCount: number;
  tokenCount: number;
  mcpConnected: boolean;
  dataSource: string;
}

export interface GraphData {
  nodes: any[];
  edges: any[];
}

export interface TabConfig {
  id: string;
  label: string;
  icon: React.ComponentType<{ size?: number; className?: string }>;
  description: string;
  buttonKey?: string;
}

export interface SettingField {
  key: string;
  label: string;
  type: 'slider' | 'toggle' | 'color' | 'nostr-button' | 'text' | 'select' | 'action-button' | 'readonly';
  path?: string;
  /**
   * Transient local-state key (not persisted to the settings store). Used for
   * task-based controls — e.g. the Analytics "Run Grouping" method/params, which
   * are one-shot inputs to POST /api/analytics/clustering/run, not settings.
   * Mutually exclusive with `path`.
   */
  localKey?: string;
  min?: number;
  max?: number;
  step?: number;
  options?: string[];
  /** Action to trigger for action-button type */
  action?: string;
  /** Optional sub-section grouping label rendered as a divider above the field */
  group?: string;
  /** Conditional visibility: only render when the named local-state key equals this value. */
  showWhen?: { localKey: string; equals: string };
  /** Setting requires power-user (authenticated) write access */
  isPowerUserOnly?: boolean;
  /** Description tooltip */
  description?: string;
}

export interface SectionConfig {
  title: string;
  fields: SettingField[];
  /** Section requires power-user (authenticated) write access */
  isPowerUserOnly?: boolean;
}

export interface SystemHealthStatus {
  websocket: 'connected' | 'connecting' | 'disconnected';
  metadata: 'loaded' | 'loading' | 'error' | 'none';
  nodes: number;
  edges: number;
  mcpSwarm: 'connected' | 'disconnected' | 'unknown';
}
