import React, { useEffect } from 'react';
import { EnterpriseDrawer } from './EnterpriseDrawer';
import { EnterpriseDrawerToggle } from './EnterpriseDrawerToggle';
import { EnterprisePanel } from './EnterprisePanel';
import { useDrawerStore } from '../store/drawerStore';

/**
 * Top-level mount for the enterprise drawer. Provides:
 *  - the drawer itself (driven by `useDrawerStore`)
 *  - a floating trigger button (bottom-right of viewport, above graph)
 *  - global keyboard shortcut (Ctrl/Cmd + .)
 *
 * Drop this once near the root of the app (alongside CommandPalette etc).
 * `EnterprisePanel` renders the same tabbed workbench content the standalone
 * full-page route shows, so no content migration is needed right now.
 */
export function EnterpriseDrawerMount() {
  const open = useDrawerStore((s) => s.open);
  const openDrawer = useDrawerStore((s) => s.openDrawer);
  const closeDrawer = useDrawerStore((s) => s.closeDrawer);
  const toggleDrawer = useDrawerStore((s) => s.toggleDrawer);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Ctrl+Shift+E / Cmd+Shift+E toggles the drawer. Avoids the Ctrl+. slot
      // which is reserved by several Linux desktop environments. Don't hijack
      // when the user is typing in an input/textarea/contenteditable.
      if (e.key?.toLowerCase() !== 'e') return;
      if (!((e.ctrlKey || e.metaKey) && e.shiftKey)) return;
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) {
        return;
      }
      e.preventDefault();
      toggleDrawer();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [toggleDrawer]);

  return (
    <>
      <div className="fixed bottom-6 right-6 z-30 pointer-events-auto">
        <div className="rounded-full bg-background/80 backdrop-blur-md border border-border/50 shadow-lg">
          <EnterpriseDrawerToggle open={open} onToggle={() => (open ? closeDrawer() : openDrawer())} />
        </div>
      </div>
      <EnterpriseDrawer open={open} onClose={closeDrawer} title="Enterprise workspace">
        <div className="p-4">
          <EnterprisePanel />
        </div>
      </EnterpriseDrawer>
    </>
  );
}

export default EnterpriseDrawerMount;
