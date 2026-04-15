import React from 'react';
import ReactDOM from 'react-dom/client';
import { EnterprisePanel } from './features/enterprise/components/EnterprisePanel';
import './styles/index.css';

function EnterprisePage() {
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
