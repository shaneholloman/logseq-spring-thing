/**
 * PathFinderPanel — PRD-005 Epic E.2
 *
 * Lets users find the shortest path between two nodes in the graph.
 * Provides autocomplete node selection, path visualization with node-kind
 * icons, and a "Highlight Path" toggle that pushes node IDs to the parent
 * for in-graph overlay rendering.
 */

import React, { useState, useMemo, useCallback, useRef, useEffect } from 'react';
import { cn } from '@/utils/classNameUtils';
import { Button } from '@/features/design-system/components/Button';
import { Badge } from '@/features/design-system/components/Badge';
import { Label } from '@/features/design-system/components/Label';
import { Switch } from '@/features/design-system/components/Switch';
import {
  Route,
  Search,
  Loader2,
  AlertTriangle,
  ArrowRight,
  FileText,
  Bot,
  Boxes,
  CircleDot,
  Sparkles,
} from 'lucide-react';

// ─── Types ────────────────────────────────────────────────────────────────────

interface PathNode {
  id: string;
  label: string;
  kind?: string;
}

export interface PathFinderPanelProps {
  nodes: PathNode[];
  onHighlightPath?: (nodeIds: string[]) => void;
  onClearHighlight?: () => void;
  className?: string;
}

interface PathResult {
  path: string[];
  length: number;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

const KIND_ICON: Record<string, React.ReactNode> = {
  page:      <FileText   className="h-3.5 w-3.5 text-blue-400" />,
  knowledge: <FileText   className="h-3.5 w-3.5 text-blue-400" />,
  agent:     <Bot        className="h-3.5 w-3.5 text-emerald-400" />,
  ontology:  <Boxes      className="h-3.5 w-3.5 text-purple-400" />,
  owl_class: <Boxes      className="h-3.5 w-3.5 text-purple-400" />,
  linked:    <CircleDot  className="h-3.5 w-3.5 text-amber-400" />,
};

function iconForKind(kind?: string): React.ReactNode {
  if (!kind) return <Sparkles className="h-3.5 w-3.5 text-muted-foreground" />;
  return KIND_ICON[kind.toLowerCase()] ?? <Sparkles className="h-3.5 w-3.5 text-muted-foreground" />;
}

// ─── Autocomplete Input ───────────────────────────────────────────────────────

interface NodeSearchInputProps {
  nodes: PathNode[];
  value: string;
  onChange: (id: string, label: string) => void;
  placeholder: string;
  label: string;
}

function NodeSearchInput({ nodes, value, onChange, placeholder, label }: NodeSearchInputProps) {
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(() => {
    if (!query) return [];
    const q = query.toLowerCase();
    return nodes.filter(n => n.label.toLowerCase().includes(q) || n.id.includes(q)).slice(0, 12);
  }, [nodes, query]);

  // Close dropdown on outside click
  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as HTMLElement)) {
        setOpen(false);
      }
    }
    document.addEventListener('mousedown', handleClick);
    return () => document.removeEventListener('mousedown', handleClick);
  }, []);

  const displayLabel = useMemo(() => {
    if (!value) return '';
    const node = nodes.find(n => n.id === value);
    return node?.label ?? value;
  }, [value, nodes]);

  return (
    <div ref={containerRef} className="relative">
      <Label className="text-xs font-medium mb-1 block">{label}</Label>
      <div className="relative">
        <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
        <input
          type="text"
          className={cn(
            'flex w-full h-9 rounded-md border border-input bg-background pl-8 pr-3 text-sm',
            'placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary',
          )}
          placeholder={placeholder}
          value={open ? query : displayLabel}
          onFocus={() => {
            setOpen(true);
            setQuery('');
          }}
          onChange={e => {
            setQuery(e.target.value);
            setOpen(true);
          }}
        />
      </div>
      {open && filtered.length > 0 && (
        <ul className="absolute z-50 mt-1 w-full max-h-48 overflow-y-auto rounded-md border bg-popover shadow-md">
          {filtered.map(node => (
            <li
              key={node.id}
              className={cn(
                'flex items-center gap-2 px-3 py-1.5 text-sm cursor-pointer hover:bg-accent',
                node.id === value && 'bg-accent',
              )}
              onMouseDown={() => {
                onChange(node.id, node.label);
                setQuery(node.label);
                setOpen(false);
              }}
            >
              {iconForKind(node.kind)}
              <span className="truncate flex-1">{node.label}</span>
              <span className="text-[10px] text-muted-foreground font-mono">#{node.id}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ─── Main Component ───────────────────────────────────────────────────────────

export function PathFinderPanel({ nodes, onHighlightPath, onClearHighlight, className }: PathFinderPanelProps) {
  const [sourceId, setSourceId] = useState('');
  const [targetId, setTargetId] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<PathResult | null>(null);
  const [highlightActive, setHighlightActive] = useState(false);

  const nodeMap = useMemo(() => new Map(nodes.map(n => [n.id, n])), [nodes]);

  const handleFind = useCallback(async () => {
    if (!sourceId || !targetId) return;
    if (sourceId === targetId) {
      setError('Source and target must be different nodes.');
      setResult(null);
      return;
    }

    setLoading(true);
    setError(null);
    setResult(null);
    setHighlightActive(false);
    onClearHighlight?.();

    try {
      const resp = await fetch('/api/pathfinding/shortest-path', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ source: sourceId, target: targetId }),
      });

      if (!resp.ok) {
        const body = await resp.text();
        throw new Error(body || `HTTP ${resp.status}`);
      }

      const data: PathResult = await resp.json();

      if (!data.path || data.path.length === 0) {
        setError('No path exists between the selected nodes.');
        return;
      }

      setResult(data);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Pathfinding request failed';
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, [sourceId, targetId, onClearHighlight]);

  const handleHighlightToggle = useCallback(
    (active: boolean) => {
      setHighlightActive(active);
      if (active && result?.path) {
        onHighlightPath?.(result.path);
      } else {
        onClearHighlight?.();
      }
    },
    [result, onHighlightPath, onClearHighlight],
  );

  return (
    <div className={cn('rounded-lg border bg-card text-card-foreground shadow-sm', className)}>
      {/* Header */}
      <div className="flex items-center gap-2 p-4 pb-2">
        <Route className="h-5 w-5 text-primary" />
        <div>
          <h3 className="text-sm font-semibold leading-none">Path Finder</h3>
          <p className="text-xs text-muted-foreground mt-0.5">Shortest path between two nodes</p>
        </div>
      </div>

      {/* Inputs */}
      <div className="px-4 py-3 space-y-3">
        <NodeSearchInput
          nodes={nodes}
          value={sourceId}
          onChange={(id) => setSourceId(id)}
          placeholder="Search source node..."
          label="Source"
        />
        <NodeSearchInput
          nodes={nodes}
          value={targetId}
          onChange={(id) => setTargetId(id)}
          placeholder="Search target node..."
          label="Target"
        />

        <Button
          onClick={handleFind}
          disabled={loading || !sourceId || !targetId}
          className="w-full"
          size="sm"
        >
          {loading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <Route className="mr-2 h-4 w-4" />}
          {loading ? 'Finding...' : 'Find Path'}
        </Button>
      </div>

      {/* Error */}
      {error && (
        <div className="mx-4 mb-3 rounded-md border border-destructive/50 bg-destructive/10 p-3 flex items-start gap-2">
          <AlertTriangle className="h-4 w-4 text-destructive mt-0.5 shrink-0" />
          <p className="text-xs text-destructive">{error}</p>
        </div>
      )}

      {/* Results */}
      {result && (
        <div className="mx-4 mb-4 rounded-md border p-3 space-y-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Badge variant="secondary">{result.path.length} nodes</Badge>
              <span className="text-xs text-muted-foreground">
                Path length: <span className="font-medium text-foreground">{result.length}</span>
              </span>
            </div>
            {(onHighlightPath || onClearHighlight) && (
              <div className="flex items-center gap-1.5">
                <Label className="text-xs">Highlight</Label>
                <Switch checked={highlightActive} onCheckedChange={handleHighlightToggle} />
              </div>
            )}
          </div>

          {/* Step-by-step path */}
          <ol className="space-y-1 max-h-52 overflow-y-auto">
            {result.path.map((nodeId, idx) => {
              const node = nodeMap.get(nodeId);
              const isLast = idx === result.path.length - 1;
              return (
                <li key={nodeId} className="flex items-center gap-1.5 text-xs">
                  <span className="shrink-0">{iconForKind(node?.kind)}</span>
                  <span className="truncate font-medium">{node?.label ?? nodeId}</span>
                  {!isLast && <ArrowRight className="h-3 w-3 text-muted-foreground shrink-0 mx-0.5" />}
                </li>
              );
            })}
          </ol>
        </div>
      )}
    </div>
  );
}
