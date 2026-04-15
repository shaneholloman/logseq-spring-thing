import React, { useState } from 'react';
import { EnterpriseNav } from './EnterpriseNav';
import { BrokerWorkbench } from '../../broker/components/BrokerWorkbench';
import { WorkflowStudio } from '../../workflows/components/WorkflowStudio';
import { MeshKpiDashboard } from '../../kpi/components/MeshKpiDashboard';
import { ConnectorPanel } from '../../connectors/components/ConnectorPanel';
import { PolicyConsole } from '../../policy/components/PolicyConsole';

export function EnterprisePanel() {
  const [activePanel, setActivePanel] = useState<string>('broker');

  return (
    <div className="flex h-full bg-background">
      <EnterpriseNav activePanel={activePanel} onPanelChange={setActivePanel} />
      <div className="flex-1 overflow-auto relative">
        <div className={activePanel === 'broker' ? 'block h-full' : 'hidden'}>
          <BrokerWorkbench />
        </div>
        <div className={activePanel === 'workflows' ? 'block h-full' : 'hidden'}>
          <WorkflowStudio />
        </div>
        <div className={activePanel === 'kpi' ? 'block h-full' : 'hidden'}>
          <MeshKpiDashboard />
        </div>
        <div className={activePanel === 'connectors' ? 'block h-full' : 'hidden'}>
          <ConnectorPanel />
        </div>
        <div className={activePanel === 'policy' ? 'block h-full' : 'hidden'}>
          <PolicyConsole />
        </div>
      </div>
    </div>
  );
}
