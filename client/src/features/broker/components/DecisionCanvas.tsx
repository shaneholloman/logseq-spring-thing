import React, { useState, useEffect } from 'react';
import {
  Card, CardContent, CardHeader, CardTitle, CardDescription,
  Badge, Button, Textarea, Label,
} from '../../design-system/components';
import { apiFetch, apiPost, ApiError } from '../../../utils/apiFetch';

interface DecisionCanvasProps {
  caseId: string;
  onDecided?: () => void;
  onBack?: () => void;
}

interface BrokerCaseDetail {
  id: string;
  title: string;
  description: string;
  priority: string;
  source: string;
  status: string;
  createdAt: string;
  updatedAt: string;
  assignedTo: string | null;
  evidence: Array<{
    itemType: string;
    sourceId: string;
    description: string;
    timestamp: string;
  }>;
}

const ACTIONS = [
  { id: 'approve', label: 'Approve', color: 'bg-emerald-500/20 text-emerald-400 hover:bg-emerald-500/30', description: 'Accept and proceed' },
  { id: 'reject', label: 'Reject', color: 'bg-red-500/20 text-red-400 hover:bg-red-500/30', description: 'Decline with reasoning' },
  { id: 'amend', label: 'Amend', color: 'bg-amber-500/20 text-amber-400 hover:bg-amber-500/30', description: 'Accept with modifications' },
  { id: 'delegate', label: 'Delegate', color: 'bg-blue-500/20 text-blue-400 hover:bg-blue-500/30', description: 'Assign to another broker' },
  { id: 'promote_as_workflow', label: 'Promote to Workflow', color: 'bg-purple-500/20 text-purple-400 hover:bg-purple-500/30', description: 'Convert to reusable pattern' },
  { id: 'mark_as_precedent', label: 'Mark as Precedent', color: 'bg-cyan-500/20 text-cyan-400 hover:bg-cyan-500/30', description: 'Flag as reference decision' },
] as const;

type ActionId = typeof ACTIONS[number]['id'];

export function DecisionCanvas({ caseId, onDecided, onBack }: DecisionCanvasProps) {
  const [caseDetail, setCaseDetail] = useState<BrokerCaseDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedAction, setSelectedAction] = useState<ActionId | null>(null);
  const [reasoning, setReasoning] = useState('');
  const [deciding, setDeciding] = useState(false);
  const [decided, setDecided] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const fetchCase = async () => {
      setLoading(true);
      setError(null);
      try {
        const data = await apiFetch<BrokerCaseDetail>(`/api/broker/cases/${caseId}`);
        if (!cancelled) setCaseDetail(data);
      } catch (err: unknown) {
        if (!cancelled) {
          const message = err instanceof ApiError ? err.message : 'Failed to load case';
          setError(message);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    fetchCase();
    return () => { cancelled = true; };
  }, [caseId]);

  const handleDecide = async () => {
    if (!selectedAction || !reasoning.trim()) return;
    setDeciding(true);
    setError(null);
    try {
      await apiPost(`/api/broker/cases/${caseId}/decide`, {
        action: selectedAction,
        reasoning: reasoning.trim(),
      });
      setDecided(true);
      onDecided?.();
    } catch (err: unknown) {
      const message = err instanceof ApiError ? err.message : 'Decision failed';
      setError(message);
    } finally {
      setDeciding(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        Loading case...
      </div>
    );
  }

  if (decided) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4">
        <div className="text-4xl" aria-hidden="true">&#10004;</div>
        <p className="text-lg font-medium text-foreground">Decision Recorded</p>
        <p className="text-sm text-muted-foreground">
          Case {caseId} &mdash; {ACTIONS.find(a => a.id === selectedAction)?.label}
        </p>
        <Button onClick={onBack} variant="outline">Back to Inbox</Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4 p-4 h-full overflow-auto">
      {/* Navigation */}
      <div className="flex items-center justify-between">
        <Button onClick={onBack} variant="ghost" size="sm">&larr; Back</Button>
        <Badge variant="outline">{caseDetail?.status || 'open'}</Badge>
      </div>

      {error && (
        <div
          role="alert"
          className="p-3 rounded-lg border border-red-500/30 bg-red-500/10 text-red-400 text-sm"
        >
          {error}
        </div>
      )}

      {/* Case Detail Header */}
      <Card>
        <CardHeader>
          <div className="flex items-center gap-2">
            <Badge variant={caseDetail?.priority === 'critical' ? 'destructive' : 'outline'}>
              {caseDetail?.priority}
            </Badge>
            <Badge variant="outline">{caseDetail?.source?.replace(/_/g, ' ')}</Badge>
          </div>
          <CardTitle className="mt-2">{caseDetail?.title}</CardTitle>
          <CardDescription>{caseDetail?.description}</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex gap-4 text-xs text-muted-foreground">
            <span>Created: {caseDetail?.createdAt}</span>
            {caseDetail?.assignedTo && <span>Assigned: {caseDetail.assignedTo}</span>}
          </div>
        </CardContent>
      </Card>

      {/* Evidence Panel */}
      {caseDetail?.evidence && caseDetail.evidence.length > 0 && (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Evidence ({caseDetail.evidence.length})</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex flex-col gap-2">
              {caseDetail.evidence.map((item, i) => (
                <div key={i} className="flex items-start gap-2 text-sm">
                  <Badge variant="outline" className="text-xs shrink-0">{item.itemType}</Badge>
                  <div className="flex flex-col min-w-0">
                    <span className="text-foreground">{item.description}</span>
                    <span className="text-xs text-muted-foreground">{item.timestamp}</span>
                  </div>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Decision Actions */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Decision</CardTitle>
          <CardDescription>Select an action and provide reasoning</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <div className="grid grid-cols-2 md:grid-cols-3 gap-2" role="radiogroup" aria-label="Decision action">
            {ACTIONS.map((action) => (
              <button
                key={action.id}
                onClick={() => setSelectedAction(action.id)}
                role="radio"
                aria-checked={selectedAction === action.id}
                className={`p-3 rounded-lg border text-left transition-all ${
                  selectedAction === action.id
                    ? `${action.color} border-current ring-1 ring-current`
                    : 'border-border hover:border-muted-foreground/30'
                }`}
              >
                <div className="font-medium text-sm">{action.label}</div>
                <div className="text-xs text-muted-foreground mt-0.5">{action.description}</div>
              </button>
            ))}
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="reasoning">Reasoning (required)</Label>
            <Textarea
              id="reasoning"
              placeholder="Explain your decision — this becomes part of the provenance record..."
              value={reasoning}
              onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => setReasoning(e.target.value)}
              rows={4}
            />
          </div>

          <Button
            onClick={handleDecide}
            disabled={!selectedAction || !reasoning.trim() || deciding}
            className="w-fit"
          >
            {deciding ? 'Recording Decision...' : 'Record Decision'}
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}
