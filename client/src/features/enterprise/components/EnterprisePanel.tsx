import React, { useState } from 'react';
import { EnterpriseNav } from './EnterpriseNav';
import { BrokerWorkbench } from '../../broker/components/BrokerWorkbench';
import { WorkflowStudio } from '../../workflows/components/WorkflowStudio';
import { MeshKpiDashboard } from '../../kpi/components/MeshKpiDashboard';
import { ConnectorPanel } from '../../connectors/components/ConnectorPanel';
import { PolicyConsole } from '../../policy/components/PolicyConsole';

export function EnterprisePanel() {
  const [activePanel, setActivePanel] = useState('broker');

  const renderPanel = () => {
    switch (activePanel) {
      case 'broker': return <BrokerWorkbench />;
      case 'workflows': return <WorkflowStudio />;
      case 'kpi': return <MeshKpiDashboard />;
      case 'connectors': return <ConnectorPanel />;
      case 'policy': return <PolicyConsole />;
      default: return null;
    }
  };

  return (
    <div className="flex h-full bg-background">
      <EnterpriseNav activePanel={activePanel} onPanelChange={setActivePanel} />
      <div className="flex-1 overflow-auto">
        {renderPanel()}
      </div>
    </div>
  );
}
