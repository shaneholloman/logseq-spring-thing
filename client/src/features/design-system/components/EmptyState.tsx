import React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';

const emptyStateVariants = cva(
  'flex flex-col items-center justify-center text-center',
  {
    variants: {
      size: {
        sm: 'py-6 gap-2',
        md: 'py-12 gap-3',
        lg: 'py-20 gap-4',
      },
    },
    defaultVariants: { size: 'md' },
  }
);

interface EmptyStateProps extends VariantProps<typeof emptyStateVariants> {
  icon?: React.ReactNode;
  title: string;
  description?: string;
  action?: React.ReactNode;
  className?: string;
}

export function EmptyState({ icon, title, description, action, size, className }: EmptyStateProps) {
  return (
    <div className={emptyStateVariants({ size, className })}>
      {icon && <div className="text-muted-foreground/50 text-4xl">{icon}</div>}
      <div>
        <p className="text-lg font-medium text-muted-foreground">{title}</p>
        {description && (
          <p className="text-sm text-muted-foreground/70 mt-1 max-w-md">{description}</p>
        )}
      </div>
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}
