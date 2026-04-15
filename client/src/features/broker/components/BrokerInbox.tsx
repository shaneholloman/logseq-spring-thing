import React, { useState, useEffect, useCallback } from 'react';
import { Card, CardContent } from '../../design-system/components';
import { Badge } from '../../design-system/components';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../../design-system/components';

interface BrokerCase {
  id: string;
  title: string;
  description: string;
  priority: string;
  source: string;
  status: string;
  createdAt: string;
  assignedTo: string | null;
}

interface BrokerInboxProps {
  onCountChange?: (count: number) => void;
}

const PRIORITY_COLORS: Record<string, string> = {
  critical: 'bg-red-500/20 text-red-400 border-red-500/30',
  high: 'bg-orange-500/20 text-orange-400 border-orange-500/30',
  medium: 'bg-yellow-500/20 text-yellow-400 border-yellow-500/30',
  low: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
};

const SOURCE_LABELS: Record<string, string> = {
  policy_violation: 'Policy',
  confidence_threshold: 'Confidence',
  trust_drift: 'Trust Drift',
  manual_submission: 'Manual',
  workflow_proposal: 'Workflow',
};

export function BrokerInbox({ onCountChange }: BrokerInboxProps) {
  const [cases, setCases] = useState<BrokerCase[]>([]);
  const [loading, setLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [selectedCase, setSelectedCase] = useState<string | null>(null);

  const fetchInbox = useCallback(async () => {
    try {
      const params = statusFilter !== 'all' ? `?status=${statusFilter}` : '';
      const response = await fetch(`/api/broker/inbox${params}`);
      const data = await response.json();
      const fetchedCases = data.cases || [];
      setCases(fetchedCases);
      onCountChange?.(fetchedCases.length);
    } catch (err) {
      console.error('Failed to fetch broker inbox:', err);
    } finally {
      setLoading(false);
    }
  }, [statusFilter, onCountChange]);

  useEffect(() => {
    fetchInbox();
    const interval = setInterval(fetchInbox, 15000);
    return () => clearInterval(interval);
  }, [fetchInbox]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-48">
        <div className="text-muted-foreground">Loading broker inbox...</div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <span className="text-sm text-muted-foreground">
          {cases.length} case{cases.length !== 1 ? 's' : ''}
        </span>
        <Select value={statusFilter} onValueChange={setStatusFilter}>
          <SelectTrigger className="w-[140px]">
            <SelectValue placeholder="Filter status" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All</SelectItem>
            <SelectItem value="open">Open</SelectItem>
            <SelectItem value="in_review">In Review</SelectItem>
            <SelectItem value="escalated">Escalated</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {cases.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center">
            <div className="text-muted-foreground">
              <p className="text-lg font-medium">No cases in inbox</p>
              <p className="text-sm mt-1">
                Submit a workflow proposal or configure policy escalation rules to generate cases.
              </p>
            </div>
          </CardContent>
        </Card>
      ) : (
        <div className="flex flex-col gap-2">
          {cases.map((brokerCase) => (
            <Card
              key={brokerCase.id}
              className={`cursor-pointer transition-colors hover:border-primary/50 ${
                selectedCase === brokerCase.id ? 'border-primary' : ''
              }`}
              onClick={() => setSelectedCase(brokerCase.id)}
            >
              <CardContent className="py-3 px-4">
                <div className="flex items-start justify-between gap-3">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium border ${PRIORITY_COLORS[brokerCase.priority] || PRIORITY_COLORS.medium}`}>
                        {brokerCase.priority}
                      </span>
                      <span className="text-xs text-muted-foreground">
                        {SOURCE_LABELS[brokerCase.source] || brokerCase.source}
                      </span>
                    </div>
                    <h3 className="font-medium text-sm text-foreground truncate">
                      {brokerCase.title}
                    </h3>
                    <p className="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                      {brokerCase.description}
                    </p>
                  </div>
                  <div className="flex flex-col items-end gap-1">
                    <Badge variant="outline" className="text-xs">
                      {brokerCase.status}
                    </Badge>
                    <span className="text-xs text-muted-foreground">
                      {brokerCase.createdAt}
                    </span>
                  </div>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
