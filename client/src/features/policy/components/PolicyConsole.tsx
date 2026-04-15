import React, { useState } from 'react';
import {
  Card, CardContent, CardHeader, CardTitle, CardDescription,
  Badge, Button, Switch,
  Tabs, TabsContent, TabsList, TabsTrigger,
  Label, Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
  Slider,
} from '../../design-system/components';

interface PolicyRule {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  action: string;
  outcome: string;
  params: Record<string, any>;
  evaluationCount: number;
}

interface EvaluationLog {
  id: string;
  ruleId: string;
  ruleName: string;
  outcome: 'allow' | 'deny' | 'escalate' | 'warn';
  reasoning: string;
  timestamp: string;
  actorId: string;
  action: string;
}

const OUTCOME_STYLES: Record<string, string> = {
  allow: 'bg-emerald-500/20 text-emerald-400 border-emerald-500/30',
  deny: 'bg-red-500/20 text-red-400 border-red-500/30',
  escalate: 'bg-amber-500/20 text-amber-400 border-amber-500/30',
  warn: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
};

const DEFAULT_RULES: PolicyRule[] = [
  { id: 'confidence_threshold', name: 'Confidence Threshold', description: 'Escalate to broker when automated confidence drops below threshold', enabled: true, action: 'workflow.promote', outcome: 'escalate', params: { threshold: 0.7 }, evaluationCount: 0 },
  { id: 'separation_of_duty', name: 'Separation of Duty', description: 'The proposer of a workflow cannot also be its approver', enabled: true, action: 'workflow.approve', outcome: 'deny', params: {}, evaluationCount: 0 },
  { id: 'domain_ownership', name: 'Domain Ownership', description: 'Only domain owners can approve workflows in their domain', enabled: true, action: 'workflow.deploy', outcome: 'escalate', params: {}, evaluationCount: 0 },
  { id: 'deployment_scope', name: 'Deployment Scope Limit', description: 'Restrict workflow deployment scope without escalation', enabled: false, action: 'workflow.deploy', outcome: 'escalate', params: { max_scope: 'department' }, evaluationCount: 0 },
  { id: 'rate_limit', name: 'Agent Rate Limit', description: 'Limit automated actions per time window per agent', enabled: false, action: '*', outcome: 'deny', params: { max_actions: 100, window_minutes: 60 }, evaluationCount: 0 },
  { id: 'escalation_cascade', name: 'Escalation Cascade', description: 'Unresolved cases older than threshold escalate to next tier', enabled: true, action: 'case.escalate', outcome: 'escalate', params: { hours_threshold: 24 }, evaluationCount: 0 },
];

export function PolicyConsole() {
  const [rules, setRules] = useState<PolicyRule[]>(DEFAULT_RULES);
  const [logs, setLogs] = useState<EvaluationLog[]>([]);
  const [activeTab, setActiveTab] = useState('rules');
  const [testAction, setTestAction] = useState('workflow.promote');
  const [testConfidence, setTestConfidence] = useState(50);
  const [testResult, setTestResult] = useState<{ outcome: string; reasoning: string } | null>(null);

  const toggleRule = (ruleId: string) => {
    setRules((prev) => prev.map((r) => (r.id === ruleId ? { ...r, enabled: !r.enabled } : r)));
  };

  const evaluateLocally = (action: string, confidence: number): { outcome: string; reasoning: string } => {
    const matching = rules.filter((r) => r.enabled && (r.action === action || r.action === '*'));
    if (matching.length === 0) return { outcome: 'allow', reasoning: 'No matching rules — default allow' };
    const deny = matching.find((r) => r.outcome === 'deny');
    if (deny) return { outcome: 'deny', reasoning: `Blocked by: ${deny.name}` };
    const escalate = matching.find((r) => r.outcome === 'escalate');
    if (escalate) {
      const threshold = escalate.params.threshold as number | undefined;
      if (threshold !== undefined && confidence >= threshold) {
        return { outcome: 'allow', reasoning: `Confidence ${confidence.toFixed(2)} >= threshold ${threshold} — ${escalate.name} passed` };
      }
      return { outcome: 'escalate', reasoning: threshold !== undefined ? `Confidence ${confidence.toFixed(2)} < threshold ${threshold} — ${escalate.name}` : `Escalation required — ${escalate.name}` };
    }
    return { outcome: 'allow', reasoning: 'All matching rules passed' };
  };

  const runTest = () => {
    const conf = testConfidence / 100;
    const result = evaluateLocally(testAction, conf);
    setTestResult(result);
    setLogs((prev) => [{ id: `eval-${Date.now()}`, ruleId: 'test', ruleName: 'Test Bench', outcome: result.outcome as EvaluationLog['outcome'], reasoning: result.reasoning, timestamp: new Date().toISOString(), actorId: 'test-user', action: testAction }, ...prev]);
  };

  return (
    <div className="h-full flex flex-col gap-4 p-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold text-foreground">Policy Console</h1>
        <Badge variant="outline">{rules.filter((r) => r.enabled).length}/{rules.length} active</Badge>
      </div>
      <Tabs value={activeTab} onValueChange={setActiveTab} className="flex-1 flex flex-col">
        <TabsList>
          <TabsTrigger value="rules">Rules</TabsTrigger>
          <TabsTrigger value="test">Test Bench</TabsTrigger>
          <TabsTrigger value="log">Evaluation Log {logs.length > 0 && `(${logs.length})`}</TabsTrigger>
        </TabsList>
        <TabsContent value="rules" className="flex-1 overflow-auto">
          <div className="flex flex-col gap-2">
            {rules.map((rule) => (
              <Card key={rule.id} className={rule.enabled ? '' : 'opacity-50'}>
                <CardContent className="py-3 px-4">
                  <div className="flex items-center justify-between">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 mb-0.5">
                        <h3 className="text-sm font-medium text-foreground">{rule.name}</h3>
                        <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium border ${OUTCOME_STYLES[rule.outcome] || ''}`}>{rule.outcome}</span>
                        <span className="text-xs text-muted-foreground">on {rule.action}</span>
                      </div>
                      <p className="text-xs text-muted-foreground">{rule.description}</p>
                      {Object.keys(rule.params).length > 0 && (
                        <div className="flex gap-2 mt-1.5">
                          {Object.entries(rule.params).map(([key, val]) => (
                            <span key={key} className="text-xs bg-muted px-1.5 py-0.5 rounded text-muted-foreground">{key}: {typeof val === 'object' ? JSON.stringify(val) : String(val)}</span>
                          ))}
                        </div>
                      )}
                    </div>
                    <Switch checked={rule.enabled} onCheckedChange={() => toggleRule(rule.id)} aria-label={`${rule.enabled ? 'Disable' : 'Enable'} ${rule.name}`} />
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        </TabsContent>
        <TabsContent value="test" className="flex-1">
          <Card>
            <CardHeader>
              <CardTitle>Policy Test Bench</CardTitle>
              <CardDescription>Simulate policy evaluation against a hypothetical action context</CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col gap-4">
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="policy-test-action">Action</Label>
                <Select value={testAction} onValueChange={setTestAction}>
                  <SelectTrigger id="policy-test-action"><SelectValue /></SelectTrigger>
                  <SelectContent>
                    <SelectItem value="workflow.promote">Promote Workflow</SelectItem>
                    <SelectItem value="workflow.approve">Approve Workflow</SelectItem>
                    <SelectItem value="workflow.deploy">Deploy Workflow</SelectItem>
                    <SelectItem value="case.escalate">Escalate Case</SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <div className="flex flex-col gap-1.5">
                <Label>Confidence: {(testConfidence / 100).toFixed(2)}</Label>
                <Slider value={[testConfidence]} onValueChange={([v]: number[]) => setTestConfidence(v)} min={0} max={100} step={5} aria-label="Confidence threshold" />
              </div>
              <Button onClick={runTest} className="w-fit">Evaluate</Button>
              {testResult && (
                <div className={`p-3 rounded-lg border ${OUTCOME_STYLES[testResult.outcome] || 'border-border'}`}>
                  <div className="flex items-center gap-2 mb-1"><span className="font-medium text-sm uppercase">{testResult.outcome}</span></div>
                  <p className="text-sm">{testResult.reasoning}</p>
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>
        <TabsContent value="log" className="flex-1 overflow-auto">
          {logs.length === 0 ? (
            <Card><CardContent className="py-12 text-center"><div className="text-4xl mb-3 opacity-50">📋</div><p className="text-lg font-medium text-muted-foreground">No evaluations yet</p><p className="text-sm text-muted-foreground/70 mt-1">Use the Test Bench to simulate, or evaluations appear as the system processes decisions.</p></CardContent></Card>
          ) : (
            <div className="flex flex-col gap-2">
              {logs.map((log) => (
                <Card key={log.id}><CardContent className="py-2.5 px-4"><div className="flex items-center justify-between"><div className="flex items-center gap-2"><span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium border ${OUTCOME_STYLES[log.outcome] || ''}`}>{log.outcome}</span><span className="text-sm text-foreground">{log.action}</span></div><span className="text-xs text-muted-foreground">{new Date(log.timestamp).toLocaleTimeString()}</span></div><p className="text-xs text-muted-foreground mt-1">{log.reasoning}</p></CardContent></Card>
              ))}
            </div>
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
