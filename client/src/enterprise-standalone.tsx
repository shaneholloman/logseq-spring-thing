import React, { Suspense, lazy } from 'react';
import ReactDOM from 'react-dom/client';
import { EnterprisePanel } from './features/enterprise/components/EnterprisePanel';
import { useHashRoute } from './hooks/useHashRoute';
import './styles/index.css';

// Lazy -- keeps drawer-fx WASM glue out of the initial bundle.
const DrawerFxDemo = lazy(() =>
  import('./features/enterprise/fx/DrawerFxDemo').then((m) => ({ default: m.DrawerFxDemo }))
);

const EnterpriseDrawerDemo = lazy(() =>
  import('./features/enterprise/components/EnterpriseDrawerDemo').then((m) => ({
    default: (m as { EnterpriseDrawerDemo?: React.ComponentType; default?: React.ComponentType })
      .EnterpriseDrawerDemo ??
      (m as { default: React.ComponentType }).default,
  }))
);

function EnterprisePage() {
  const route = useHashRoute();
  if (route === '/fx-demo') {
    return (
      <Suspense fallback={<div style={{ color: '#aaa', padding: 24 }}>loading fx…</div>}>
        <DrawerFxDemo />
      </Suspense>
    );
  }
  if (route === '/drawer-demo') {
    return (
      <Suspense fallback={<div style={{ color: '#aaa', padding: 24 }}>loading drawer…</div>}>
        <EnterpriseDrawerDemo />
      </Suspense>
    );
  }
  return (
    <div className="h-screen w-screen bg-background text-foreground">
      <EnterprisePanel />
    </div>
  );
}

const root = ReactDOM.createRoot(document.getElementById('root')!);
root.render(
  <React.StrictMode>
    <EnterprisePage />
  </React.StrictMode>
);
