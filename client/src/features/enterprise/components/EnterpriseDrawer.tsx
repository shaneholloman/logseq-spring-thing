import React, {
  ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
} from 'react';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { X } from 'lucide-react';
import { useDrawerFx } from '../fx/useDrawerFx';

/**
 * Sliding workspace drawer that extends the enterprise control centre
 * rightward, overlaying the live graph with a frosted-glass alpha blend.
 *
 * Notes:
 * - Scrim is intentionally subtle (`bg-foreground/5`). Clicks on the scrim do
 *   NOT dismiss — the drawer is a workspace, not a transient menu. Only the
 *   close button or the Escape key close it.
 * - Honours `prefers-reduced-motion`: skips the horizontal slide and uses a
 *   plain fade instead.
 * - Focus trap is hand-rolled (Tab / Shift-Tab across focusable descendants)
 *   to avoid pulling in `@radix-ui/react-focus-scope` just for this prototype.
 */
export interface EnterpriseDrawerProps {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
  title?: string;
  /** Optional label read by assistive tech. Defaults to `title`. */
  ariaLabel?: string;
}

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'textarea:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

const SPRING = { type: 'spring' as const, stiffness: 380, damping: 34 };
const FADE = { duration: 0.18, ease: 'easeOut' as const };

export function EnterpriseDrawer({
  open,
  onClose,
  children,
  title = 'Enterprise workspace',
  ariaLabel,
}: EnterpriseDrawerProps) {
  const prefersReducedMotion = useReducedMotion();
  const panelRef = useRef<HTMLDivElement | null>(null);
  const previouslyFocusedRef = useRef<HTMLElement | null>(null);
  const fxCanvasRef = useRef<HTMLCanvasElement | null>(null);

  // WASM-powered flow-field particle background behind the drawer's frosted
  // glass. No-ops under `prefers-reduced-motion` or if the WASM chunk fails
  // to load — in which case the radial-gradient fallback below is the backdrop.
  useDrawerFx(open, fxCanvasRef);

  // Escape key closes.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  // Focus management: remember opener, focus first focusable, restore on close.
  useEffect(() => {
    if (!open) return;
    previouslyFocusedRef.current = document.activeElement as HTMLElement | null;
    const panel = panelRef.current;
    if (panel) {
      const focusables = panel.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR);
      (focusables[0] ?? panel).focus();
    }
    return () => {
      previouslyFocusedRef.current?.focus?.();
    };
  }, [open]);

  // Manual focus trap.
  const onPanelKeyDown = useCallback((e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== 'Tab') return;
    const panel = panelRef.current;
    if (!panel) return;
    const focusables = Array.from(
      panel.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
    ).filter((el) => !el.hasAttribute('data-focus-guard'));
    if (focusables.length === 0) {
      e.preventDefault();
      panel.focus();
      return;
    }
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement;
    if (e.shiftKey && active === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  }, []);

  const panelVariants = useMemo(
    () =>
      prefersReducedMotion
        ? {
            hidden: { opacity: 0, x: 0 },
            visible: { opacity: 1, x: 0, transition: FADE },
            exit: { opacity: 0, x: 0, transition: FADE },
          }
        : {
            hidden: { opacity: 0, x: '100%' },
            visible: { opacity: 1, x: 0, transition: SPRING },
            exit: { opacity: 0, x: '100%', transition: SPRING },
          },
    [prefersReducedMotion],
  );

  return (
    <AnimatePresence>
      {open && (
        <div
          className="fixed inset-0 z-40 pointer-events-none"
          data-testid="enterprise-drawer"
        >
          {/* Subtle scrim over the graph. Non-dismissive by design. */}
          <motion.div
            aria-hidden="true"
            className="absolute inset-0 bg-foreground/5 pointer-events-auto"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={FADE}
          />

          {/* Sliding panel */}
          <motion.aside
            ref={panelRef}
            role="dialog"
            aria-modal="true"
            aria-label={ariaLabel ?? title}
            tabIndex={-1}
            onKeyDown={onPanelKeyDown}
            className={[
              'absolute right-0 top-0 h-full pointer-events-auto',
              'w-full md:w-[68vw] lg:w-[56vw] xl:w-[48vw]',
              'backdrop-blur-2xl bg-background/70',
              'border-l border-border/40 shadow-2xl',
              'flex flex-col outline-none',
              'focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:ring-inset',
            ].join(' ')}
            variants={panelVariants}
            initial="hidden"
            animate="visible"
            exit="exit"
          >
            {/* WASM flow-field background, z-0 beneath the panel's frosted glass. */}
            <canvas
              ref={fxCanvasRef}
              aria-hidden="true"
              className="absolute inset-0 w-full h-full pointer-events-none opacity-60 mix-blend-screen"
              style={{ zIndex: 0 }}
            />

            <header className="relative z-10 flex items-center justify-between px-5 py-3 border-b border-border/40 bg-card/40">
              <h2 className="text-sm font-semibold text-foreground tracking-tight">
                {title}
              </h2>
              <button
                type="button"
                onClick={onClose}
                aria-label="Close enterprise drawer"
                className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground hover:bg-muted/50 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
              >
                <X className="h-4 w-4" aria-hidden="true" />
              </button>
            </header>

            <div className="relative z-10 flex-1 overflow-y-auto overscroll-contain px-5 py-4">
              {children}
            </div>
          </motion.aside>
        </div>
      )}
    </AnimatePresence>
  );
}
