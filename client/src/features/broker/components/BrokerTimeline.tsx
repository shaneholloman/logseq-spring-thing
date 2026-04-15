import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle } from '../../design-system/components';
import { Badge } from '../../design-system/components';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../../design-system/components';
import { apiFetch, ApiError } from '../../../utils/apiFetch';

interface DecisionRecord {
  id: string;
  caseId: string;
  caseTitle: string;
  action: string;
  reasoning: string;
  decidedBy: string;
  decidedAt: string;
  outcome?: string;
}

const ACTION_STYLES: Record<string, { color: string; dotColor: string }> = {
  approve: { color: 'text-emerald-400', dotColor: 'bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.4)]' },
  reject: { color: 'text-red-400', dotColor: 'bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.4)]' },
  amend: { color: 'text-amber-400', dotColor: 'bg-amber-500 shadow-[0_0_8px_rgba(245,158,11,0.4)]' },
  delegate: { color: 'text-blue-400', dotColor: 'bg-blue-500 shadow-[0_0_8px_rgba(59,130,246,0.4)]' },
  promote_as_workflow: { color: 'text-purple-400', dotColor: 'bg-purple-500 shadow-[0_0_8px_rgba(168,85,247,0.4)]' },
  mark_as_precedent: { color: 'text-cyan-400', dotColor: 'bg-cyan-500 shadow-[0_0_8px_rgba(6,182,212,0.4)]' },
  request_more_evidence: { color: 'text-gray-400', dotColor: 'bg-gray-500' },
};

const ACTION_LABELS: Record<string, string> = {
  approve: 'Approved',
  reject: 'Rejected',
  amend: 'Amended',
  delegate: 'Delegated',
  promote_as_workflow: 'Promoted to Workflow',
  mark_as_precedent: 'Marked as Precedent',
  request_more_evidence: 'Requested Evidence',
};

export function BrokerTimeline() {
  const [decisions, setDecisions] = useState<DecisionRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState('all');

  useEffect(() => {
    const fetchDecisions = async () => {
      try {
        setError(null);
        const data = await apiFetch<{ cases: any[] }>('/api/broker/inbox?status=decided');
        const records: DecisionRecord[] = (data.cases || [])
          .filter((c: any) => c.status === 'decided')
          .map((c: any) => ({
            id: `dec-${c.id}`,
            caseId: c.id,
            caseTitle: c.title,
            action: c.lastAction || 'approve',
            reasoning: c.reasoning || '',
            decidedBy: c.assignedTo || 'broker',
            decidedAt: c.updatedAt || c.createdAt,
          }));
        setDecisions(records);
      } catch (err) {
        const message = err instanceof ApiError ? err.message : 'Network error';
        setError(message);
        console.error('Failed to fetch decisions:', err);
      } finally {
        setLoading(false);
      }
    };

    fetchDecisions();
  }, []);

  const filtered = filter === 'all'
    ? decisions
    : decisions.filter((d) => d.action === filter);

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>Decision Timeline</CardTitle>
          <Select value={filter} onValueChange={setFilter}>
            <SelectTrigger className="w-[160px]">
              <SelectValue placeholder="Filter action" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Actions</SelectItem>
              <SelectItem value="approve">Approved</SelectItem>
              <SelectItem value="reject">Rejected</SelectItem>
              <SelectItem value="amend">Amended</SelectItem>
              <SelectItem value="delegate">Delegated</SelectItem>
              <SelectItem value="promote_as_workflow">Promoted</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </CardHeader>
      <CardContent>
        {error && (
          <div className="p-3 rounded-lg border border-red-500/30 bg-red-500/10 text-red-400 text-sm mb-4">
            {error}
          </div>
        )}
        {loading ? (
          <div className="flex items-center justify-center py-8">
            <div className="text-muted-foreground">Loading timeline...</div>
          </div>
        ) : filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-center">
            <div className="text-4xl mb-3 opacity-50">{'\u2696\uFE0F'}</div>
            <p className="text-lg font-medium text-muted-foreground">No decisions yet</p>
            <p className="text-sm text-muted-foreground/70 mt-1 max-w-sm">
              Review cases in the Inbox tab. Each decision creates an auditable record here with full provenance.
            </p>
          </div>
        ) : (
          <div className="relative">
            {filtered.map((decision, index) => {
              const style = ACTION_STYLES[decision.action] || ACTION_STYLES.approve;
              return (
                <div key={decision.id} className="relative pl-8 pb-6 last:pb-0">
                  {index < filtered.length - 1 && (
                    <div className="absolute left-[11px] top-6 bottom-0 w-px bg-border" />
                  )}
                  <div className={`absolute left-1 top-1.5 h-3 w-3 rounded-full ${style.dotColor}`} />
                  <div>
                    <div className="flex items-center gap-2 flex-wrap">
                      <Badge variant="outline" className={`text-xs ${style.color}`}>
                        {ACTION_LABELS[decision.action] || decision.action}
                      </Badge>
                      <span className="text-xs text-muted-foreground">{decision.decidedAt}</span>
                      <span className="text-xs text-muted-foreground">by {decision.decidedBy}</span>
                    </div>
                    <h4 className="text-sm font-medium text-foreground mt-1">{decision.caseTitle}</h4>
                    {decision.reasoning && (
                      <p className="text-xs text-muted-foreground mt-0.5 italic">&ldquo;{decision.reasoning}&rdquo;</p>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
