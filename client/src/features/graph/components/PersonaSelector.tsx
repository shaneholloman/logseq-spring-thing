import React, { useState, useCallback } from 'react';
import { Globe, Code, BookOpen, Server, Boxes, Check } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import { cn } from '../../../utils/classNameUtils';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface Persona {
  id: string;
  label: string;
  icon: LucideIcon;
  description: string;
  /** Node kinds visible under this persona. Empty = ALL visible. */
  nodeKinds: string[];
  /** Edge categories visible under this persona. Empty = ALL visible. */
  edgeCategories: string[];
}

// ---------------------------------------------------------------------------
// Built-in personas
// ---------------------------------------------------------------------------

export const PERSONAS: Persona[] = [
  {
    id: 'full-graph',
    label: 'Full Graph',
    icon: Globe,
    description: 'Everything visible — all node kinds and edge categories.',
    nodeKinds: [],
    edgeCategories: [],
  },
  {
    id: 'code-architecture',
    label: 'Code Architecture',
    icon: Code,
    description: 'Functions, modules, classes, interfaces and their structural relationships.',
    nodeKinds: ['Function', 'Module', 'Class', 'Interface', 'Variable'],
    edgeCategories: ['Structural', 'Behavioural', 'DataFlow', 'Dependencies'],
  },
  {
    id: 'knowledge-explorer',
    label: 'Knowledge Explorer',
    icon: BookOpen,
    description: 'Pages, blocks, and concepts connected by knowledge and semantic edges.',
    nodeKinds: ['Page', 'Block', 'Concept'],
    edgeCategories: ['Knowledge', 'Semantic'],
  },
  {
    id: 'infrastructure-view',
    label: 'Infrastructure View',
    icon: Server,
    description: 'Services, containers, databases, queues, caches, and network topology.',
    nodeKinds: [
      'Service', 'Container', 'Database', 'Queue',
      'Cache', 'Gateway', 'LoadBalancer', 'Cdn',
    ],
    edgeCategories: ['Infrastructure', 'Dependencies'],
  },
  {
    id: 'domain-model',
    label: 'Domain Model',
    icon: Boxes,
    description: 'Entities, value objects, aggregates, and ontology classes with domain semantics.',
    nodeKinds: [
      'Entity', 'ValueObject', 'Aggregate',
      'OntologyClass', 'OntologyIndividual',
    ],
    edgeCategories: ['Domain', 'Semantic'],
  },
];

// ---------------------------------------------------------------------------
// Colour map — one accent per persona for the active ring & icon tint
// ---------------------------------------------------------------------------

const PERSONA_COLORS: Record<string, string> = {
  'full-graph':           'bg-slate-500/10  hover:bg-slate-500/20  border-slate-500/30',
  'code-architecture':    'bg-blue-500/10   hover:bg-blue-500/20   border-blue-500/30',
  'knowledge-explorer':   'bg-amber-500/10  hover:bg-amber-500/20  border-amber-500/30',
  'infrastructure-view':  'bg-emerald-500/10 hover:bg-emerald-500/20 border-emerald-500/30',
  'domain-model':         'bg-purple-500/10 hover:bg-purple-500/20 border-purple-500/30',
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export interface PersonaSelectorProps {
  /** Currently active persona id. Defaults to 'full-graph'. */
  activeId?: string;
  /** Fired when the user selects a persona. */
  onPersonaChange: (persona: Persona) => void;
  className?: string;
}

export const PersonaSelector: React.FC<PersonaSelectorProps> = ({
  activeId: controlledActiveId,
  onPersonaChange,
  className,
}) => {
  const [internalActiveId, setInternalActiveId] = useState<string>('full-graph');
  const activeId = controlledActiveId ?? internalActiveId;

  const handleSelect = useCallback(
    (persona: Persona) => {
      setInternalActiveId(persona.id);
      onPersonaChange(persona);
    },
    [onPersonaChange],
  );

  return (
    <div className={cn('flex flex-wrap gap-2', className)}>
      {PERSONAS.map((persona) => {
        const Icon = persona.icon;
        const isActive = activeId === persona.id;
        const colorCls = PERSONA_COLORS[persona.id] ?? PERSONA_COLORS['full-graph'];

        return (
          <button
            key={persona.id}
            type="button"
            onClick={() => handleSelect(persona)}
            title={persona.description}
            className={cn(
              'relative flex items-center gap-2 px-3 py-2 rounded-lg border transition-all',
              colorCls,
              isActive && 'ring-2 ring-primary shadow-md',
            )}
          >
            <Icon className="w-4 h-4 shrink-0" />
            <span className="text-sm font-medium whitespace-nowrap">
              {persona.label}
            </span>
            {isActive && <Check className="w-3 h-3 ml-0.5 shrink-0" />}
          </button>
        );
      })}
    </div>
  );
};

export default PersonaSelector;
