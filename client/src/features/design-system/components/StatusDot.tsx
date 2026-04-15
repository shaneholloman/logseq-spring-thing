import React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';

const statusDotVariants = cva(
  'inline-block rounded-full',
  {
    variants: {
      status: {
        active: 'bg-emerald-500 shadow-[0_0_6px_rgba(16,185,129,0.6)]',
        warning: 'bg-amber-500 shadow-[0_0_6px_rgba(245,158,11,0.6)]',
        error: 'bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.6)]',
        inactive: 'bg-gray-500',
        processing: 'bg-blue-500 shadow-[0_0_6px_rgba(59,130,246,0.6)] animate-pulse',
      },
      size: {
        sm: 'h-1.5 w-1.5',
        md: 'h-2.5 w-2.5',
        lg: 'h-3.5 w-3.5',
      },
    },
    defaultVariants: {
      status: 'inactive',
      size: 'md',
    },
  }
);

interface StatusDotProps extends VariantProps<typeof statusDotVariants> {
  label?: string;
  className?: string;
}

export function StatusDot({ status, size, label, className }: StatusDotProps) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span
        className={statusDotVariants({ status, size, className })}
        role="status"
        aria-label={label || (status as string)}
      />
      {label && <span className="text-xs text-muted-foreground">{label}</span>}
    </span>
  );
}
