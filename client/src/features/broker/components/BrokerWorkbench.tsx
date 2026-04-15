import React, { useState } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../../design-system/components';
import { Badge } from '../../design-system/components';
import { BrokerInbox } from './BrokerInbox';
import { CaseSubmitForm } from './CaseSubmitForm';
import { BrokerTimeline } from './BrokerTimeline';
import { DecisionCanvas } from './DecisionCanvas';

export function BrokerWorkbench() {
  const [activeTab, setActiveTab] = useState('inbox');
  const [caseCount, setCaseCount] = useState(0);
  const [selectedCaseId, setSelectedCaseId] = useState<string | null>(null);

  const handleCaseSelect = (caseId: string) => {
    setSelectedCaseId(caseId);
  };

  const handleBackToInbox = () => {
    setSelectedCaseId(null);
  };

  const handleDecided = () => {
    // Decision recorded -- stay on confirmation screen until user clicks Back
  };

  return (
    <div className="h-full flex flex-col gap-4 p-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h1 className="text-xl font-semibold text-foreground">Judgment Broker</h1>
          {caseCount > 0 && (
            <Badge variant="destructive">{caseCount} open</Badge>
          )}
        </div>
      </div>

      {selectedCaseId ? (
        <DecisionCanvas
          caseId={selectedCaseId}
          onDecided={handleDecided}
          onBack={handleBackToInbox}
        />
      ) : (
        <Tabs value={activeTab} onValueChange={setActiveTab} className="flex-1 flex flex-col">
          <TabsList>
            <TabsTrigger value="inbox">Inbox</TabsTrigger>
            <TabsTrigger value="submit">Submit Case</TabsTrigger>
            <TabsTrigger value="timeline">Timeline</TabsTrigger>
          </TabsList>

          <TabsContent value="inbox" className="flex-1">
            <BrokerInbox onCountChange={setCaseCount} onCaseSelect={handleCaseSelect} />
          </TabsContent>

          <TabsContent value="submit" className="flex-1">
            <CaseSubmitForm onSubmitted={() => setActiveTab('inbox')} />
          </TabsContent>

          <TabsContent value="timeline" className="flex-1">
            <BrokerTimeline />
          </TabsContent>
        </Tabs>
      )}
    </div>
  );
}
