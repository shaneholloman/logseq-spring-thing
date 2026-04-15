import React, { useState, useEffect } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '../../design-system/components';
import { Badge } from '../../design-system/components';
import { Button } from '../../design-system/components';
import { Input } from '../../design-system/components';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../../design-system/components';
import { Textarea } from '../../design-system/components';
import { Label } from '../../design-system/components';
import { apiFetch, apiPost, ApiError } from '../../../utils/apiFetch';

interface WorkflowProposal {
  id: string;
  title: string;
  description: string;
  status: string;
  version: number;
  createdAt: string;
}

interface WorkflowPattern {
  id: string;
  title: string;
  description: string;
  adoptionCount: number;
  deployedAt: string;
}

const STATUS_COLORS: Record<string, string> = {
  draft: 'bg-gray-500/20 text-gray-400',
  submitted: 'bg-blue-500/20 text-blue-400',
  under_review: 'bg-yellow-500/20 text-yellow-400',
  approved: 'bg-green-500/20 text-green-400',
  deployed: 'bg-emerald-500/20 text-emerald-400',
  archived: 'bg-gray-500/20 text-gray-500',
};

export function WorkflowStudio() {
  const [proposals, setProposals] = useState<WorkflowProposal[]>([]);
  const [patterns, setPatterns] = useState<WorkflowPattern[]>([]);
  const [activeTab, setActiveTab] = useState('proposals');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // New proposal form state
  const [newTitle, setNewTitle] = useState('');
  const [newDescription, setNewDescription] = useState('');
  const [showForm, setShowForm] = useState(false);
  const [promotingId, setPromotingId] = useState<string | null>(null);

  useEffect(() => {
    const fetchData = async () => {
      try {
        setError(null);
        const [proposalsData, patternsData] = await Promise.all([
          apiFetch<{ proposals: WorkflowProposal[] }>('/api/workflows/proposals'),
          apiFetch<{ patterns: WorkflowPattern[] }>('/api/workflows/patterns'),
        ]);
        setProposals(proposalsData.proposals || []);
        setPatterns(patternsData.patterns || []);
      } catch (err) {
        const message = err instanceof ApiError ? err.message : 'Network error';
        setError(message);
        console.error('Failed to fetch workflow data:', err);
      } finally {
        setLoading(false);
      }
    };
    fetchData();
  }, []);

  const handlePromote = async (proposal: WorkflowProposal) => {
    setPromotingId(proposal.id);
    setError(null);
    try {
      await apiPost(`/api/workflows/proposals/${proposal.id}/promote`, {});
      setProposals((prev) =>
        prev.map((p) => (p.id === proposal.id ? { ...p, status: 'deployed' } : p))
      );
      // Re-fetch patterns to include the newly promoted one
      const patternsData = await apiFetch<{ patterns: WorkflowPattern[] }>('/api/workflows/patterns');
      setPatterns(patternsData.patterns || []);
    } catch (err) {
      const message = err instanceof ApiError ? err.message : 'Promotion failed';
      setError(message);
      console.error('Failed to promote proposal:', err);
    } finally {
      setPromotingId(null);
    }
  };

  const handleCreateProposal = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newTitle.trim()) return;
    try {
      setError(null);
      const data = await apiPost<WorkflowProposal>('/api/workflows/proposals', {
        title: newTitle, description: newDescription,
      });
      setProposals((prev) => [{ ...data, description: newDescription, createdAt: new Date().toISOString() }, ...prev]);
      setNewTitle('');
      setNewDescription('');
      setShowForm(false);
    } catch (err) {
      const message = err instanceof ApiError ? err.message : 'Network error';
      setError(message);
      console.error('Failed to create proposal:', err);
    }
  };

  return (
    <div className="h-full flex flex-col gap-4 p-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold text-foreground">Workflow Studio</h1>
        <Button onClick={() => setShowForm(!showForm)} variant={showForm ? 'outline' : 'default'}>
          {showForm ? 'Cancel' : 'New Proposal'}
        </Button>
      </div>

      {error && (
        <div className="p-3 rounded-lg border border-red-500/30 bg-red-500/10 text-red-400 text-sm">
          {error}
        </div>
      )}

      {showForm && (
        <Card>
          <CardHeader>
            <CardTitle>New Workflow Proposal</CardTitle>
            <CardDescription>Describe a discovered workflow pattern for governance review</CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleCreateProposal} className="flex flex-col gap-3">
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="wf-title">Title</Label>
                <Input
                  id="wf-title"
                  placeholder="e.g., Automated compliance check for data exports"
                  value={newTitle}
                  onChange={(e) => setNewTitle(e.target.value)}
                  required
                />
              </div>
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="wf-desc">Description</Label>
                <Textarea
                  id="wf-desc"
                  placeholder="Steps involved, teams affected, expected benefit..."
                  value={newDescription}
                  onChange={(e) => setNewDescription(e.target.value)}
                  rows={3}
                />
              </div>
              <Button type="submit" disabled={!newTitle.trim()} className="w-fit">
                Submit Proposal
              </Button>
            </form>
          </CardContent>
        </Card>
      )}

      <Tabs value={activeTab} onValueChange={setActiveTab} className="flex-1 flex flex-col">
        <TabsList>
          <TabsTrigger value="proposals">
            Proposals {proposals.length > 0 && `(${proposals.length})`}
          </TabsTrigger>
          <TabsTrigger value="patterns">
            Patterns {patterns.length > 0 && `(${patterns.length})`}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="proposals" className="flex-1">
          {proposals.length === 0 ? (
            <Card>
              <CardContent className="py-12 text-center text-muted-foreground">
                <p className="text-lg font-medium">No proposals yet</p>
                <p className="text-sm mt-1">Create a proposal to start the workflow governance loop.</p>
              </CardContent>
            </Card>
          ) : (
            <div className="flex flex-col gap-2">
              {proposals.map((proposal) => (
                <Card key={proposal.id} className="hover:border-primary/50 transition-colors">
                  <CardContent className="py-3 px-4">
                    <div className="flex items-center justify-between">
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 mb-1">
                          <span className={`px-2 py-0.5 rounded-full text-xs font-medium ${STATUS_COLORS[proposal.status] || ''}`}>
                            {proposal.status}
                          </span>
                          <span className="text-xs text-muted-foreground">v{proposal.version}</span>
                        </div>
                        <h3 className="font-medium text-sm text-foreground truncate">{proposal.title}</h3>
                      </div>
                      {proposal.status === 'approved' && (
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={promotingId === proposal.id}
                          onClick={() => handlePromote(proposal)}
                          className="shrink-0 ml-2"
                        >
                          {promotingId === proposal.id ? 'Promoting...' : 'Promote to Pattern'}
                        </Button>
                      )}
                    </div>
                  </CardContent>
                </Card>
              ))}
            </div>
          )}
        </TabsContent>

        <TabsContent value="patterns" className="flex-1">
          {patterns.length === 0 ? (
            <Card>
              <CardContent className="py-12 text-center text-muted-foreground">
                <p className="text-lg font-medium">No deployed patterns</p>
                <p className="text-sm mt-1">Approved proposals become reusable patterns when promoted.</p>
              </CardContent>
            </Card>
          ) : (
            <div className="flex flex-col gap-2">
              {patterns.map((pattern) => (
                <Card key={pattern.id}>
                  <CardContent className="py-3 px-4">
                    <h3 className="font-medium text-sm">{pattern.title}</h3>
                    <div className="flex items-center gap-2 mt-1">
                      <Badge variant="outline" className="text-xs">{pattern.adoptionCount} teams</Badge>
                    </div>
                  </CardContent>
                </Card>
              ))}
            </div>
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
