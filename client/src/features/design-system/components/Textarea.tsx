import * as React from 'react';
import { cva, type VariantProps } from 'class-variance-authority';
import { cn } from '../../../utils/classNameUtils';

const textareaVariants = cva(
  'flex w-full rounded-lg bg-background text-foreground transition-all duration-200 placeholder:text-muted-foreground focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50 resize-y',
  {
    variants: {
      variant: {
        default: 'border border-input focus:border-primary focus:ring-2 focus:ring-primary/20',
        filled: 'bg-secondary border border-transparent focus:border-primary focus:bg-background',
        flushed: 'border-0 border-b-2 border-input rounded-none px-0 focus:border-primary',
        ghost: 'border-0 focus:bg-accent/5',
        outlined: 'border-2 border-input focus:border-primary',
      },
      size: {
        sm: 'min-h-[60px] px-2.5 py-1.5 text-sm',
        default: 'min-h-[80px] px-3 py-2 text-sm',
        lg: 'min-h-[120px] px-4 py-3 text-base',
        xl: 'min-h-[160px] px-5 py-4 text-lg',
      },
      state: {
        default: '',
        error: 'border-destructive focus:border-destructive focus:ring-destructive/20',
        success: 'border-emerald-500 focus:border-emerald-500 focus:ring-emerald-500/20',
        warning: 'border-amber-500 focus:border-amber-500 focus:ring-amber-500/20',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
      state: 'default',
    },
  }
);

export interface TextareaProps
  extends Omit<React.TextareaHTMLAttributes<HTMLTextAreaElement>, 'size'>,
    VariantProps<typeof textareaVariants> {
  label?: string;
  error?: string;
  success?: string;
  warning?: string;
  helper?: string;
  maxLength?: number;
  showCount?: boolean;
}

const Textarea = React.forwardRef<HTMLTextAreaElement, TextareaProps>(
  (
    {
      className,
      variant,
      size,
      state: stateProp,
      label,
      error,
      success,
      warning,
      helper,
      maxLength,
      showCount = false,
      value,
      defaultValue,
      onChange,
      ...props
    },
    ref
  ) => {
    const [charCount, setCharCount] = React.useState(() => {
      const initial = (value ?? defaultValue ?? '') as string;
      return initial.length;
    });

    const state = error ? 'error' : success ? 'success' : warning ? 'warning' : stateProp || 'default';
    const message = error || success || warning || helper;

    const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      setCharCount(e.target.value.length);
      onChange?.(e);
    };

    React.useEffect(() => {
      if (value !== undefined) {
        setCharCount((value as string).length);
      }
    }, [value]);

    return (
      <div className="w-full">
        {label && (
          <label
            htmlFor={props.id}
            className={cn(
              'block text-sm font-medium mb-1.5',
              state === 'error' && 'text-destructive',
              state === 'success' && 'text-emerald-500',
              state === 'warning' && 'text-amber-500'
            )}
          >
            {label}
          </label>
        )}

        <textarea
          ref={ref}
          className={cn(textareaVariants({ variant, size, state }), className)}
          value={value}
          defaultValue={defaultValue}
          onChange={handleChange}
          maxLength={maxLength}
          aria-invalid={state === 'error'}
          aria-describedby={message ? `${props.id}-message` : undefined}
          {...props}
        />

        <div className="flex items-center justify-between mt-1.5">
          {message ? (
            <p
              id={`${props.id}-message`}
              className={cn(
                'text-sm',
                state === 'error' && 'text-destructive',
                state === 'success' && 'text-emerald-500',
                state === 'warning' && 'text-amber-500',
                state === 'default' && 'text-muted-foreground'
              )}
            >
              {message}
            </p>
          ) : (
            <span />
          )}

          {showCount && (
            <span className="text-xs text-muted-foreground">
              {charCount}{maxLength ? `/${maxLength}` : ''}
            </span>
          )}
        </div>
      </div>
    );
  }
);

Textarea.displayName = 'Textarea';

export { Textarea, textareaVariants };
