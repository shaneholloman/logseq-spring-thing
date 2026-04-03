import React, { useState, useRef, useEffect, useCallback } from 'react';

interface CommandInputProps {
  isCollapsed: boolean;
}

interface StatusLine {
  id: number;
  text: string;
  timestamp: number;
}

// ---------------------------------------------------------------------------
// Validation & Parsing
// ---------------------------------------------------------------------------

function validateCommand(cmd: string): { valid: boolean; message: string } {
  const lower = cmd.toLowerCase();

  const blocked = ['exec', 'eval', 'require', 'import', 'fetch(', 'delete', 'drop', 'truncate', 'rm ', 'sudo'];
  for (const b of blocked) {
    if (lower.includes(b)) {
      return { valid: false, message: 'This interface only accepts view and graph configuration commands.' };
    }
  }

  const accepted = [
    'cluster', 'hull', 'show', 'hide', 'zoom', 'repel', 'spring', 'force',
    'damping', 'physics', 'layout', 'edge', 'node', 'label', 'glow', 'bloom',
    'opacity', 'size', 'color', 'colour', 'domain', 'type', 'quality', 'filter',
    'reset', 'default', 'increase', 'decrease', 'more', 'less', 'enable', 'disable',
    'ontology', 'knowledge', 'agent', 'strength', 'spread', 'tight', 'compact',
    'separate', 'group', 'visibility', 'sync', 'local', 'analytics', 'metric',
    'semantic', 'overwhelm', 'clutter', 'fewer', 'bigger', 'read', 'only',
    'focus', 'find', 'search', 'dag', 'radial', 'hierarchy', 'tree',
  ];

  const hasRelevantKeyword = accepted.some(kw => lower.includes(kw));
  if (!hasRelevantKeyword) {
    return {
      valid: false,
      message: 'Please describe a view or graph configuration change. Try: "show clusters with hulls" or "increase repulsion"',
    };
  }

  return { valid: true, message: '' };
}

interface SettingsAction {
  description: string;
  endpoint?: string;
  method?: string;
  body?: Record<string, unknown>;
  localAction?: () => void;
}

function parseCommandToActions(cmd: string): SettingsAction[] {
  const lower = cmd.toLowerCase();
  const actions: SettingsAction[] = [];

  // Cluster hulls
  if (lower.includes('hull') || lower.includes('cluster hull')) {
    if (lower.includes('hide') || lower.includes('disable') || lower.includes('off')) {
      actions.push({
        description: 'Disabling cluster hulls',
        localAction: () => {
          const ss = (window as any).useSettingsStore?.getState();
          ss?.updateSettings?.((draft: any) => {
            if (draft.visualisation?.clusterHulls) draft.visualisation.clusterHulls.enabled = false;
          });
        },
      });
    } else {
      actions.push({
        description: 'Enabling cluster hulls',
        localAction: () => {
          const ss = (window as any).useSettingsStore?.getState();
          ss?.updateSettings?.((draft: any) => {
            if (!draft.visualisation) draft.visualisation = {};
            if (!draft.visualisation.clusterHulls) draft.visualisation.clusterHulls = {};
            draft.visualisation.clusterHulls.enabled = true;
            draft.visualisation.clusterHulls.opacity = 0.10;
          });
        },
      });
    }
  }

  // Physics: repulsion / spread / separate
  if (lower.includes('repul') || lower.includes('spread') || lower.includes('separate')) {
    const increase = lower.includes('increase') || lower.includes('more') || lower.includes('spread');
    const value = increase ? 400 : 100;
    actions.push({
      description: `Setting repulsion to ${value}`,
      endpoint: '/api/settings/physics',
      method: 'PUT',
      body: { repelK: value },
    });
  }

  // Physics: spring / attract / tight / compact
  if (lower.includes('spring') || lower.includes('attract') || lower.includes('tight') || lower.includes('compact')) {
    const increase = lower.includes('increase') || lower.includes('more') || lower.includes('tight') || lower.includes('compact');
    const value = increase ? 5.0 : 1.0;
    actions.push({
      description: `Setting spring strength to ${value}`,
      endpoint: '/api/settings/physics',
      method: 'PUT',
      body: { springK: value },
    });
  }

  // Physics: damping
  if (lower.includes('damp')) {
    const value = lower.includes('increase') || lower.includes('more') ? 0.8 : 0.3;
    actions.push({
      description: `Setting damping to ${value}`,
      endpoint: '/api/settings/physics',
      method: 'PUT',
      body: { damping: value },
    });
  }

  // Visibility: knowledge
  if (lower.includes('knowledge')) {
    const show = !lower.includes('hide');
    actions.push({
      description: `${show ? 'Showing' : 'Hiding'} knowledge nodes`,
      localAction: () => {
        const ss = (window as any).useSettingsStore?.getState();
        ss?.set?.('visualisation.graphs.logseq.nodes.nodeTypeVisibility.knowledge', show);
      },
    });
  }

  // Visibility: ontology
  if (lower.includes('ontology') && (lower.includes('show') || lower.includes('hide'))) {
    const show = !lower.includes('hide');
    actions.push({
      description: `${show ? 'Showing' : 'Hiding'} ontology nodes`,
      localAction: () => {
        const ss = (window as any).useSettingsStore?.getState();
        ss?.set?.('visualisation.graphs.logseq.nodes.nodeTypeVisibility.ontology', show);
      },
    });
  }

  // Quality filter
  if (lower.includes('quality') && (lower.match(/\d+\.?\d*/) || lower.includes('high'))) {
    const match = lower.match(/(\d+\.?\d*)/);
    const threshold = match ? parseFloat(match[1]) : 0.8;
    const normalized = threshold > 1 ? threshold / 100 : threshold;
    actions.push({
      description: `Filtering to quality >= ${normalized}`,
      localAction: () => {
        const ss = (window as any).useSettingsStore?.getState();
        ss?.set?.('nodeFilter.enabled', true);
        ss?.set?.('nodeFilter.filterByQuality', true);
        ss?.set?.('nodeFilter.qualityThreshold', normalized);
      },
    });
  }

  // Node opacity
  if (lower.includes('opacity') || lower.includes('transparent') || lower.includes('visible')) {
    const match = lower.match(/(\d+\.?\d*)/);
    let value = match ? parseFloat(match[1]) : 0.8;
    if (value > 1) value = value / 100;
    actions.push({
      description: `Setting node opacity to ${value}`,
      localAction: () => {
        const ss = (window as any).useSettingsStore?.getState();
        ss?.set?.('visualisation.graphs.logseq.nodes.opacity', value);
      },
    });
  }

  // Labels
  if (lower.includes('label')) {
    const show = !lower.includes('hide') && !lower.includes('off');
    actions.push({
      description: `${show ? 'Showing' : 'Hiding'} labels`,
      localAction: () => {
        const ss = (window as any).useSettingsStore?.getState();
        ss?.set?.('visualisation.graphs.logseq.labels.enableLabels', show);
      },
    });
  }

  // Reset to defaults
  if (lower.includes('reset') || lower.includes('default')) {
    actions.push({
      description: 'Resetting physics to defaults',
      endpoint: '/api/settings/physics',
      method: 'PUT',
      body: { repelK: 200, springK: 2.0, damping: 0.5, restLength: 80, maxVelocity: 200 },
    });
  }

  // Bloom / glow
  if (lower.includes('glow') || lower.includes('bloom')) {
    const enable = !lower.includes('off') && !lower.includes('disable');
    actions.push({
      description: `${enable ? 'Enabling' : 'Disabling'} bloom`,
      localAction: () => {
        const ss = (window as any).useSettingsStore?.getState();
        ss?.updateSettings?.((draft: any) => {
          if (draft.visualisation?.bloom) draft.visualisation.bloom.enabled = enable;
          if (draft.visualisation?.glow) draft.visualisation.glow.enabled = enable;
        });
      },
    });
  }

  // Clustering algorithm
  if (lower.includes('kmeans') || lower.includes('louvain') || lower.includes('spectral')) {
    const algo = lower.includes('kmeans') ? 'kmeans' : lower.includes('louvain') ? 'louvain' : 'spectral';
    actions.push({
      description: `Running ${algo} clustering`,
      endpoint: '/api/clustering/configure',
      method: 'POST',
      body: { algorithm: algo, numClusters: 6, resolution: 1.0, iterations: 30, exportAssignments: true, autoUpdate: false },
    });
    actions.push({
      description: 'Starting clustering computation',
      endpoint: '/api/clustering/start',
      method: 'POST',
      body: {},
    });
  }

  // Semantic clusters — "create semantic clusters", "group by topic", "cluster"
  if ((lower.includes('semantic') && lower.includes('cluster')) || lower.includes('group by')) {
    actions.push({
      description: 'Enabling semantic layout forces + Louvain clustering',
      endpoint: '/api/settings/physics',
      method: 'PUT',
      body: { clusteringAlgorithm: 'louvain', clusterCount: 8, clusteringResolution: 1.0, clusteringIterations: 50 },
    });
    actions.push({
      description: 'Enabling cluster visualization',
      localAction: () => {
        const ss = (window as any).useSettingsStore?.getState();
        ss?.updateSettings?.((draft: any) => {
          if (!draft.qualityGates) draft.qualityGates = {};
          draft.qualityGates.showClusters = true;
          draft.qualityGates.semanticForces = true;
          if (!draft.visualisation) draft.visualisation = {};
          if (!draft.visualisation.clusterHulls) draft.visualisation.clusterHulls = {};
          draft.visualisation.clusterHulls.enabled = true;
          draft.visualisation.clusterHulls.opacity = 0.08;
        });
      },
    });
  }

  // "less overwhelming labels", "fewer labels", "reduce label clutter"
  if (lower.includes('overwhelm') || lower.includes('clutter') || lower.includes('fewer label') || lower.includes('less label')) {
    actions.push({
      description: 'Reducing label visibility for clarity',
      localAction: () => {
        const ss = (window as any).useSettingsStore?.getState();
        ss?.updateSettings?.((draft: any) => {
          const labels = draft.visualisation?.graphs?.logseq?.labels;
          if (labels) {
            labels.labelDistanceThreshold = Math.max(100, (labels.labelDistanceThreshold || 1200) * 0.5);
            labels.desktopFontSize = Math.max(0.15, (labels.desktopFontSize || 0.35) * 0.7);
            labels.showMetadata = false;
          }
        });
      },
    });
  }

  // "more labels", "bigger labels", "I can't read the labels"
  if ((lower.includes('more') || lower.includes('bigger') || lower.includes('read')) && lower.includes('label')) {
    actions.push({
      description: 'Increasing label visibility',
      localAction: () => {
        const ss = (window as any).useSettingsStore?.getState();
        ss?.updateSettings?.((draft: any) => {
          const labels = draft.visualisation?.graphs?.logseq?.labels;
          if (labels) {
            labels.labelDistanceThreshold = Math.min(2000, (labels.labelDistanceThreshold || 500) * 1.5);
            labels.desktopFontSize = Math.min(2.0, (labels.desktopFontSize || 0.35) * 1.4);
            labels.enableLabels = true;
          }
        });
      },
    });
  }

  // "only knowledge and agents" / "only show knowledge graph and agents"
  if (lower.includes('only') && (lower.includes('knowledge') || lower.includes('agent'))) {
    const showKnowledge = lower.includes('knowledge');
    const showAgent = lower.includes('agent');
    const showOntology = lower.includes('ontology');
    actions.push({
      description: `Showing: ${[showKnowledge && 'knowledge', showOntology && 'ontology', showAgent && 'agents'].filter(Boolean).join(', ')}`,
      localAction: () => {
        const ss = (window as any).useSettingsStore?.getState();
        ss?.updateSettings?.((draft: any) => {
          const vis = draft.visualisation?.graphs?.logseq?.nodes?.nodeTypeVisibility;
          if (vis) {
            vis.knowledge = showKnowledge;
            vis.ontology = showOntology;
            vis.agent = showAgent;
          }
        });
      },
    });
  }

  // "layout force directed" / "layout DAG" / "layout radial"
  if (lower.includes('layout') && (lower.includes('dag') || lower.includes('radial') || lower.includes('hierarchy') || lower.includes('tree'))) {
    const mode = lower.includes('radial') ? 'dag-radial' : lower.includes('left') ? 'dag-leftright' : 'dag-topdown';
    actions.push({
      description: `Switching to ${mode} layout`,
      localAction: () => {
        const ss = (window as any).useSettingsStore?.getState();
        ss?.updateSettings?.((draft: any) => {
          if (!draft.qualityGates) draft.qualityGates = {};
          draft.qualityGates.layoutMode = mode;
          draft.qualityGates.semanticForces = true;
        });
      },
    });
  }

  // "focus on X" / "find X" — zoom to node by label search
  if (lower.includes('focus') || lower.includes('find') || lower.includes('search')) {
    const searchTerm = cmd.replace(/^(focus|find|search)\s*(on|for)?\s*/i, '').trim();
    if (searchTerm.length > 1) {
      actions.push({
        description: `Searching for "${searchTerm}" — use the graph to navigate`,
        localAction: () => {
          // Dispatch a custom event that the graph can listen for
          window.dispatchEvent(new CustomEvent('visionflow:search', { detail: { query: searchTerm } }));
        },
      });
    }
  }

  // Fallback
  if (actions.length === 0) {
    actions.push({
      description: 'Try: "create semantic clusters", "less overwhelming labels", "only show knowledge and agents", "layout DAG radial", "increase repulsion", "show cluster hulls"',
      localAction: () => {},
    });
  }

  return actions;
}

async function executeAction(action: SettingsAction): Promise<void> {
  if (action.localAction) {
    action.localAction();
  }
  if (action.endpoint) {
    const response = await fetch(action.endpoint, {
      method: action.method || 'GET',
      headers: { 'Content-Type': 'application/json' },
      body: action.body ? JSON.stringify(action.body) : undefined,
    });
    if (!response.ok) {
      throw new Error(`API error: ${response.status}`);
    }
  }
}

// ---------------------------------------------------------------------------
// StatusText — retro green scanline fade
// ---------------------------------------------------------------------------

const StatusText: React.FC<{ text: string; timestamp: number }> = ({ text, timestamp }) => {
  const [opacity, setOpacity] = useState(1);

  useEffect(() => {
    const fadeStart = setTimeout(() => {
      const interval = setInterval(() => {
        setOpacity(prev => {
          const next = prev - 0.02;
          if (next <= 0) {
            clearInterval(interval);
            return 0;
          }
          return next;
        });
      }, 50);
      return () => clearInterval(interval);
    }, 2000);
    return () => clearTimeout(fadeStart);
  }, [timestamp]);

  if (opacity <= 0) return null;

  return (
    <div style={{
      color: `rgba(0, 255, 65, ${opacity})`,
      fontSize: '11px',
      fontFamily: '"Courier New", monospace',
      padding: '2px 4px',
      textShadow: `0 0 4px rgba(0, 255, 65, ${opacity * 0.5})`,
      backgroundImage: opacity > 0.5
        ? 'repeating-linear-gradient(0deg, transparent, transparent 1px, rgba(0,0,0,0.1) 1px, rgba(0,0,0,0.1) 2px)'
        : 'none',
      transition: 'opacity 0.1s linear',
    }}>
      {text}
    </div>
  );
};

// ---------------------------------------------------------------------------
// CommandInput — main exported component
// ---------------------------------------------------------------------------

export const CommandInput: React.FC<CommandInputProps> = ({ isCollapsed }) => {
  const [command, setCommand] = useState('');
  const [queue, setQueue] = useState<string[]>([]);
  const [isProcessing, setIsProcessing] = useState(false);
  const [statusLines, setStatusLines] = useState<StatusLine[]>([]);
  const inputRef = useRef<HTMLInputElement>(null);
  const statusIdRef = useRef(0);

  const addStatus = useCallback((text: string) => {
    const id = ++statusIdRef.current;
    setStatusLines(prev => [...prev.slice(-4), { id, text, timestamp: Date.now() }]);
  }, []);

  const processCommand = useCallback(async (cmd: string) => {
    setIsProcessing(true);
    addStatus(`Processing: ${cmd}`);

    try {
      const validationResult = validateCommand(cmd);
      if (!validationResult.valid) {
        addStatus(validationResult.message);
        setIsProcessing(false);
        return;
      }

      const actions = parseCommandToActions(cmd);

      for (const action of actions) {
        addStatus(action.description);
        await executeAction(action);
        await new Promise(r => setTimeout(r, 300));
      }

      addStatus('Configuration applied');
    } catch (error) {
      addStatus(`Error: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }

    setIsProcessing(false);
    setCommand('');
  }, [addStatus]);

  // Process queue when current command finishes
  useEffect(() => {
    if (!isProcessing && queue.length > 0) {
      const next = queue[0];
      setQueue(prev => prev.slice(1));
      processCommand(next);
    }
  }, [isProcessing, queue, processCommand]);

  const handleSubmit = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    if (!command.trim()) return;

    if (isProcessing) {
      setQueue(prev => [...prev, command.trim()]);
      addStatus(`Queued: ${command.trim()}`);
      setCommand('');
    } else {
      processCommand(command.trim());
    }
  }, [command, isProcessing, addStatus, processCommand]);

  if (!isCollapsed) return null;

  return (
    <div style={{
      position: 'fixed',
      top: '12px',
      left: '58px',
      width: '25vw',
      minWidth: '300px',
      maxWidth: '500px',
      zIndex: 1000,
      pointerEvents: 'auto',
    }}>
      <form onSubmit={handleSubmit}>
        <input
          ref={inputRef}
          type="text"
          value={command}
          onChange={(e) => setCommand(e.target.value)}
          placeholder={isProcessing ? 'Command queued...' : 'Configure view...'}
          style={{
            width: '100%',
            padding: '8px 12px',
            background: 'rgba(0, 0, 0, 0.75)',
            border: '1px solid rgba(255, 255, 255, 0.15)',
            borderRadius: '4px',
            color: '#ffffff',
            fontSize: '13px',
            fontFamily: 'monospace',
            outline: 'none',
            backdropFilter: 'blur(8px)',
          }}
        />
      </form>

      <div style={{ marginTop: '4px', minHeight: '20px' }}>
        {statusLines.map(line => (
          <StatusText key={line.id} text={line.text} timestamp={line.timestamp} />
        ))}
      </div>
    </div>
  );
};

export default CommandInput;
