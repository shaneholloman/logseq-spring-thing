import React from 'react';

interface EnterpriseNavProps {
  activePanel: string;
  onPanelChange: (panel: string) => void;
}

const NAV_ITEMS = [
  { id: 'broker', label: 'Broker', icon: '\u2696\uFE0F' },
  { id: 'workflows', label: 'Workflows', icon: '\uD83D\uDD04' },
  { id: 'kpi', label: 'KPIs', icon: '\uD83D\uDCCA' },
  { id: 'connectors', label: 'Connectors', icon: '\uD83D\uDD0C' },
  { id: 'policy', label: 'Policy', icon: '\uD83D\uDCCB' },
];

export function EnterpriseNav({ activePanel, onPanelChange }: EnterpriseNavProps) {
  return (
    <nav className="flex flex-col gap-1 p-2 w-48 border-r border-border bg-card/50">
      <div className="px-3 py-2 text-xs font-semibold text-muted-foreground uppercase tracking-wider">
        Enterprise
      </div>
      {NAV_ITEMS.map(({ id, label, icon }) => (
        <button
          key={id}
          onClick={() => onPanelChange(id)}
          className={`flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors ${
            activePanel === id
              ? 'bg-primary/10 text-primary font-medium'
              : 'text-muted-foreground hover:text-foreground hover:bg-muted/50'
          }`}
        >
          <span>{icon}</span>
          <span>{label}</span>
        </button>
      ))}
    </nav>
  );
}
