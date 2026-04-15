import React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';

const timelineItemVariants = cva(
  'relative pl-8 pb-6 last:pb-0',
  {
    variants: {
      status: {
        default: '',
        success: '',
        warning: '',
        error: '',
        info: '',
      },
    },
    defaultVariants: { status: 'default' },
  }
);

const dotColors: Record<string, string> = {
  default: 'bg-muted-foreground/50',
  success: 'bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.4)]',
  warning: 'bg-amber-500 shadow-[0_0_8px_rgba(245,158,11,0.4)]',
  error: 'bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.4)]',
  info: 'bg-blue-500 shadow-[0_0_8px_rgba(59,130,246,0.4)]',
};

export interface TimelineItem {
  id: string;
  title: string;
  description?: string;
  timestamp: string;
  status?: 'default' | 'success' | 'warning' | 'error' | 'info';
  icon?: React.ReactNode;
  metadata?: Record<string, string>;
}

interface TimelineProps {
  items: TimelineItem[];
  className?: string;
}

export function Timeline({ items, className }: TimelineProps) {
  if (items.length === 0) return null;

  return (
    <ol aria-label="Timeline" className={className}>
      {items.map((item, index) => (
        <li key={item.id} className={timelineItemVariants({ status: item.status })}>
          {/* Vertical line */}
          {index < items.length - 1 && (
            <div className="absolute left-[11px] top-6 bottom-0 w-px bg-border" aria-hidden="true" />
          )}
          {/* Dot */}
          <div
            className={`absolute left-1 top-1.5 h-3 w-3 rounded-full ${dotColors[item.status || 'default']}`}
            aria-hidden="true"
          />
          {/* Content */}
          <div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-medium text-foreground">{item.title}</span>
              <span className="text-xs text-muted-foreground">{item.timestamp}</span>
            </div>
            {item.description && (
              <p className="text-sm text-muted-foreground mt-0.5">{item.description}</p>
            )}
            {item.metadata && Object.keys(item.metadata).length > 0 && (
              <div className="flex flex-wrap gap-2 mt-1.5">
                {Object.entries(item.metadata).map(([key, value]) => (
                  <span key={key} className="inline-flex items-center px-2 py-0.5 rounded-full text-xs bg-muted text-muted-foreground">
                    {key}: {value}
                  </span>
                ))}
              </div>
            )}
          </div>
        </li>
      ))}
    </ol>
  );
}
