import React from 'react';
import { SlidersHorizontal } from 'lucide-react';

export interface EnterpriseDrawerToggleProps {
  open: boolean;
  onToggle: () => void;
  label?: string;
}

/**
 * Small trigger button styled like the existing control-panel tab buttons
 * (see `EnterpriseNav`). Deliberately compact — meant to live in the
 * control centre header, not as a floating action button.
 */
export function EnterpriseDrawerToggle({
  open,
  onToggle,
  label = 'Enterprise',
}: EnterpriseDrawerToggleProps) {
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-expanded={open}
      aria-controls="enterprise-drawer"
      className={[
        'flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors',
        'focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/40',
        open
          ? 'bg-primary/10 text-primary font-medium'
          : 'text-muted-foreground hover:text-foreground hover:bg-muted/50',
      ].join(' ')}
    >
      <SlidersHorizontal className="h-4 w-4" aria-hidden="true" />
      <span>{label}</span>
    </button>
  );
}
