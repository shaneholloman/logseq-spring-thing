import React, { useState, useCallback, useMemo } from 'react';
import {
  Layers,
  Zap,
  ArrowRight,
  Link,
  Brain,
  Server,
  Puzzle,
  BookOpen,
  Eye,
  EyeOff,
  ChevronDown,
  ChevronUp,
  type LucideIcon,
} from 'lucide-react';
import { cn } from '../../../utils/classNameUtils';

// ---------------------------------------------------------------------------
// Types & constants
// ---------------------------------------------------------------------------

export type EdgeCategory =
  | 'structural'
  | 'behavioural'
  | 'dataflow'
  | 'dependencies'
  | 'semantic'
  | 'infrastructure'
  | 'domain'
  | 'knowledge';

export interface EdgeCategoryDef {
  id: EdgeCategory;
  label: string;
  icon: LucideIcon;
  color: string;
  bgActive: string;
  description: string;
}

export const EDGE_CATEGORIES: EdgeCategoryDef[] = [
  { id: 'structural',     label: 'Structural',     icon: Layers,    color: 'text-blue-400',    bgActive: 'bg-blue-500/20 border-blue-500/40',    description: 'Contains, InheritsFrom, Implements, ComposedOf, Nests' },
  { id: 'behavioural',    label: 'Behavioural',    icon: Zap,       color: 'text-yellow-400',  bgActive: 'bg-yellow-500/20 border-yellow-500/40',description: 'Calls, Overrides, Triggers, Subscribes' },
  { id: 'dataflow',       label: 'Data Flow',      icon: ArrowRight,color: 'text-green-400',   bgActive: 'bg-green-500/20 border-green-500/40',  description: 'ReadsFrom, WritesTo, TransformsTo, Pipes' },
  { id: 'dependencies',   label: 'Dependencies',   icon: Link,      color: 'text-orange-400',  bgActive: 'bg-orange-500/20 border-orange-500/40',description: 'DependsOn, Imports, Requires, Enables' },
  { id: 'semantic',       label: 'Semantic',        icon: Brain,     color: 'text-purple-400',  bgActive: 'bg-purple-500/20 border-purple-500/40',description: 'SubClassOf, InstanceOf, EquivalentTo, DisjointWith, SameAs' },
  { id: 'infrastructure', label: 'Infrastructure',  icon: Server,    color: 'text-cyan-400',    bgActive: 'bg-cyan-500/20 border-cyan-500/40',    description: 'DeploysTo, RoutesTo, ReplicatesTo, Monitors' },
  { id: 'domain',         label: 'Domain',          icon: Puzzle,    color: 'text-pink-400',    bgActive: 'bg-pink-500/20 border-pink-500/40',    description: 'HasPart, BridgesTo, Fulfills, Constrains' },
  { id: 'knowledge',      label: 'Knowledge',       icon: BookOpen,  color: 'text-emerald-400', bgActive: 'bg-emerald-500/20 border-emerald-500/40',description: 'WikiLink, BlockRef, BlockParent, TaggedWith, CitedBy' },
];

const ALL_IDS = EDGE_CATEGORIES.map(c => c.id);

function allVisible(): Record<EdgeCategory, boolean> {
  return Object.fromEntries(ALL_IDS.map(id => [id, true])) as Record<EdgeCategory, boolean>;
}

function noneVisible(): Record<EdgeCategory, boolean> {
  return Object.fromEntries(ALL_IDS.map(id => [id, false])) as Record<EdgeCategory, boolean>;
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface EdgeCategoryFilterProps {
  /** Called whenever the set of visible categories changes. */
  onFilterChange: (visible: EdgeCategory[]) => void;
  /** Optional per-category edge counts to display as badges. */
  counts?: Partial<Record<EdgeCategory, number>>;
  /** Start in compact (icons-only) mode. Default false. */
  defaultCompact?: boolean;
  className?: string;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const EdgeCategoryFilter: React.FC<EdgeCategoryFilterProps> = ({
  onFilterChange,
  counts,
  defaultCompact = false,
  className,
}) => {
  const [visibility, setVisibility] = useState<Record<EdgeCategory, boolean>>(allVisible);
  const [compact, setCompact] = useState(defaultCompact);

  const visibleList = useMemo(
    () => ALL_IDS.filter(id => visibility[id]),
    [visibility],
  );

  const allOn = visibleList.length === ALL_IDS.length;
  const noneOn = visibleList.length === 0;

  const applyChange = useCallback(
    (next: Record<EdgeCategory, boolean>) => {
      setVisibility(next);
      onFilterChange(ALL_IDS.filter(id => next[id]));
    },
    [onFilterChange],
  );

  const toggle = useCallback(
    (id: EdgeCategory) => {
      applyChange({ ...visibility, [id]: !visibility[id] });
    },
    [visibility, applyChange],
  );

  const setAll = useCallback(() => applyChange(allVisible()), [applyChange]);
  const setNone = useCallback(() => applyChange(noneVisible()), [applyChange]);

  return (
    <div className={cn('flex flex-col gap-2', className)}>
      {/* Header row */}
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-semibold uppercase tracking-wider text-gray-400">
          Edge Categories
        </span>
        <div className="flex items-center gap-1">
          <button
            onClick={setAll}
            disabled={allOn}
            className={cn(
              'px-2 py-0.5 rounded text-[10px] font-medium transition-colors',
              allOn
                ? 'text-gray-600 cursor-default'
                : 'text-gray-400 hover:text-white hover:bg-white/10',
            )}
          >
            All
          </button>
          <button
            onClick={setNone}
            disabled={noneOn}
            className={cn(
              'px-2 py-0.5 rounded text-[10px] font-medium transition-colors',
              noneOn
                ? 'text-gray-600 cursor-default'
                : 'text-gray-400 hover:text-white hover:bg-white/10',
            )}
          >
            None
          </button>
          <button
            onClick={() => setCompact(prev => !prev)}
            className="p-1 rounded text-gray-400 hover:text-white hover:bg-white/10 transition-colors"
            title={compact ? 'Expand labels' : 'Compact mode'}
          >
            {compact ? <ChevronDown className="w-3 h-3" /> : <ChevronUp className="w-3 h-3" />}
          </button>
        </div>
      </div>

      {/* Category toggles */}
      <div className={cn('flex flex-wrap gap-1.5', compact && 'gap-1')}>
        {EDGE_CATEGORIES.map(cat => {
          const Icon = cat.icon;
          const active = visibility[cat.id];
          const count = counts?.[cat.id];

          return (
            <button
              key={cat.id}
              onClick={() => toggle(cat.id)}
              title={`${cat.label}: ${cat.description}`}
              className={cn(
                'relative flex items-center gap-1.5 rounded-lg border transition-all',
                compact ? 'px-2 py-1.5' : 'px-3 py-1.5',
                active
                  ? cn(cat.bgActive, cat.color)
                  : 'border-gray-700/50 text-gray-600 hover:text-gray-400 hover:border-gray-600/50',
              )}
            >
              <Icon className={cn('w-3.5 h-3.5 flex-shrink-0', active ? cat.color : 'text-current')} />

              {!compact && (
                <span className="text-xs font-medium whitespace-nowrap">{cat.label}</span>
              )}

              {count !== undefined && count > 0 && (
                <span
                  className={cn(
                    'ml-0.5 min-w-[18px] px-1 py-px rounded-full text-center text-[9px] font-bold leading-tight',
                    active ? 'bg-white/15 text-current' : 'bg-gray-700/60 text-gray-500',
                  )}
                >
                  {count > 999 ? `${Math.floor(count / 1000)}k` : count}
                </span>
              )}

              {/* Active indicator dot */}
              {active && (
                <span className="absolute -top-0.5 -right-0.5 w-1.5 h-1.5 rounded-full bg-current opacity-80" />
              )}
            </button>
          );
        })}
      </div>

      {/* Summary line */}
      <div className="flex items-center gap-1 text-[10px] text-gray-500">
        {allOn ? (
          <><Eye className="w-3 h-3" /> All categories visible</>
        ) : noneOn ? (
          <><EyeOff className="w-3 h-3" /> All categories hidden</>
        ) : (
          <><Eye className="w-3 h-3" /> {visibleList.length} of {ALL_IDS.length} visible</>
        )}
      </div>
    </div>
  );
};

export default EdgeCategoryFilter;
