import React, { useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../../design-system/components';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../../design-system/components';
import { Badge } from '../../design-system/components';
import { BrokerInbox } from './BrokerInbox';
import { CaseSubmitForm } from './CaseSubmitForm';
import { BrokerTimeline } from './BrokerTimeline';

export function BrokerWorkbench() {
  const [activeTab, setActiveTab] = useState('inbox');
  const [caseCount, setCaseCount] = useState(0);

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

      <Tabs value={activeTab} onValueChange={setActiveTab} className="flex-1 flex flex-col">
        <TabsList>
          <TabsTrigger value="inbox">Inbox</TabsTrigger>
          <TabsTrigger value="submit">Submit Case</TabsTrigger>
          <TabsTrigger value="timeline">Timeline</TabsTrigger>
        </TabsList>

        <TabsContent value="inbox" className="flex-1">
          <BrokerInbox onCountChange={setCaseCount} />
        </TabsContent>

        <TabsContent value="submit" className="flex-1">
          <CaseSubmitForm onSubmitted={() => setActiveTab('inbox')} />
        </TabsContent>

        <TabsContent value="timeline" className="flex-1">
          <BrokerTimeline />
        </TabsContent>
      </Tabs>
    </div>
  );
}
