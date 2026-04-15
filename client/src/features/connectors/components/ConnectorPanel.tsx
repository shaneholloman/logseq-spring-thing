import React, { useCallback, useEffect, useState } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '../../design-system/components';
import { Badge } from '../../design-system/components';
import { Button } from '../../design-system/components';
import { Input } from '../../design-system/components';
import { Label } from '../../design-system/components';
import { Switch } from '../../design-system/components';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '../../design-system/components';
import { apiFetch, apiPost } from '../../../utils/apiFetch';

interface Connector {
  id: string;
  type: string;
  name: string;
  status: 'active' | 'paused' | 'error' | 'configuring';
  lastSync: string | null;
  signalCount: number;
}

interface Signal {
  id: string;
  connectorId: string;
  type: string;
  summary: string;
  detectedAt: string;
  strength: number;
}

interface ListConnectorsResponse {
  success: boolean;
  data: {
    connectors: Array<{
      id: string;
      connectorType: string;
      name: string;
      status: string;
      config: unknown;
      lastSync: string | null;
      createdAt: string;
      createdBy: string;
      signalCount: number;
    }>;
    total: number;
  };
}

interface CreateConnectorResponse {
  success: boolean;
  data: {
    id: string;
    status: string;
    message: string;
  };
}

interface DeleteConnectorResponse {
  success: boolean;
  data: {
    deleted: boolean;
  };
}

const STATUS_CONFIG: Record<string, { color: string; dot: string }> = {
  active: { color: 'text-emerald-400', dot: 'bg-emerald-500 shadow-[0_0_6px_rgba(16,185,129,0.6)]' },
  paused: { color: 'text-gray-400', dot: 'bg-gray-500' },
  error: { color: 'text-red-400', dot: 'bg-red-500 shadow-[0_0_6px_rgba(239,68,68,0.6)]' },
  configuring: { color: 'text-blue-400', dot: 'bg-blue-500 animate-pulse' },
};

export function ConnectorPanel() {
  const [connectors, setConnectors] = useState<Connector[]>([]);
  const [signals, setSignals] = useState<Signal[]>([]);
  const [showSetup, setShowSetup] = useState(false);
  const [activeTab, setActiveTab] = useState('connectors');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [setupOrg, setSetupOrg] = useState('');
  const [setupRepos, setSetupRepos] = useState('');
  const [setupRedaction, setSetupRedaction] = useState(true);

  const fetchConnectors = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const resp = await apiFetch<ListConnectorsResponse>('/api/connectors');
      const mapped: Connector[] = resp.data.connectors.map((c) => ({
        id: c.id,
        type: c.connectorType,
        name: c.name,
        status: c.status as Connector['status'],
        lastSync: c.lastSync,
        signalCount: c.signalCount,
      }));
      setConnectors(mapped);
    } catch (err) {
      console.error('Failed to fetch connectors:', err);
      setError(err instanceof Error ? err.message : 'Failed to fetch connectors');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchConnectors();
  }, [fetchConnectors]);

  const handleCreateConnector = async () => {
    if (!setupOrg.trim()) return;

    try {
      setError(null);
      const repos = setupRepos
        .split(',')
        .map((r) => r.trim())
        .filter(Boolean);

      await apiPost<CreateConnectorResponse>('/api/connectors', {
        connectorType: 'github',
        name: `GitHub: ${setupOrg}`,
        config: {
          organisation: setupOrg,
          repositories: repos.length > 0 ? repos : null,
          piiRedaction: setupRedaction,
        },
      });

      setShowSetup(false);
      setSetupOrg('');
      setSetupRepos('');
      await fetchConnectors();
    } catch (err) {
      console.error('Failed to create connector:', err);
      setError(err instanceof Error ? err.message : 'Failed to create connector');
    }
  };

  const handleDeleteConnector = async (id: string) => {
    try {
      setError(null);
      await apiFetch<DeleteConnectorResponse>(`/api/connectors/${id}`, {
        method: 'DELETE',
      });
      await fetchConnectors();
    } catch (err) {
      console.error('Failed to delete connector:', err);
      setError(err instanceof Error ? err.message : 'Failed to delete connector');
    }
  };

  return (
    <div className="h-full flex flex-col gap-4 p-4">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold text-foreground">Discovery Connectors</h1>
        <div className="flex items-center gap-2">
          <Button onClick={fetchConnectors} variant="outline" disabled={loading}>
            {loading ? 'Loading...' : 'Refresh'}
          </Button>
          <Button onClick={() => setShowSetup(!showSetup)} variant={showSetup ? 'outline' : 'default'}>
            {showSetup ? 'Cancel' : 'Add Connector'}
          </Button>
        </div>
      </div>

      {error && (
        <div className="bg-red-900/20 border border-red-500/30 rounded-md px-3 py-2 text-sm text-red-400">
          {error}
        </div>
      )}

      {showSetup && (
        <Card className="border-primary/30">
          <CardHeader>
            <CardTitle>Configure GitHub Connector</CardTitle>
            <CardDescription>
              Connect to GitHub Issues and PRs to discover workflow patterns. Tier 1 connector per ADR-044.
            </CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-4">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="conn-org">GitHub Organisation</Label>
              <Input
                id="conn-org"
                placeholder="e.g., DreamLab-AI"
                value={setupOrg}
                onChange={(e) => setSetupOrg(e.target.value)}
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="conn-repos">Repositories (comma-separated, empty = all)</Label>
              <Input
                id="conn-repos"
                placeholder="e.g., VisionClaw, ontology-data"
                value={setupRepos}
                onChange={(e) => setSetupRepos(e.target.value)}
              />
            </div>
            <div className="flex items-center justify-between">
              <div>
                <Label htmlFor="pii-redaction">PII Redaction</Label>
                <p className="text-xs text-muted-foreground">Strip emails and names before storage</p>
              </div>
              <Switch id="pii-redaction" checked={setupRedaction} onCheckedChange={setSetupRedaction} />
            </div>
            <Button onClick={handleCreateConnector} disabled={!setupOrg.trim()} className="w-fit">
              Create Connector
            </Button>
          </CardContent>
        </Card>
      )}

      <Tabs value={activeTab} onValueChange={setActiveTab} className="flex-1 flex flex-col">
        <TabsList>
          <TabsTrigger value="connectors">
            Connectors {connectors.length > 0 && `(${connectors.length})`}
          </TabsTrigger>
          <TabsTrigger value="signals">
            Signal Feed {signals.length > 0 && `(${signals.length})`}
          </TabsTrigger>
        </TabsList>

        <TabsContent value="connectors" className="flex-1">
          {connectors.length === 0 ? (
            <Card>
              <CardContent className="py-12 text-center">
                <div className="text-4xl mb-3 opacity-50">{'\uD83D\uDD0C'}</div>
                <p className="text-lg font-medium text-muted-foreground">No connectors configured</p>
                <p className="text-sm text-muted-foreground/70 mt-1 max-w-sm mx-auto">
                  Add a GitHub connector to start discovering workflow patterns from Issues and PRs.
                </p>
                <Button onClick={() => setShowSetup(true)} className="mt-4" variant="outline">
                  Add First Connector
                </Button>
              </CardContent>
            </Card>
          ) : (
            <div className="flex flex-col gap-2">
              {connectors.map((conn) => {
                const cfg = STATUS_CONFIG[conn.status] ?? STATUS_CONFIG.configuring;
                return (
                  <Card key={conn.id}>
                    <CardContent className="py-3 px-4">
                      <div className="flex items-center justify-between">
                        <div className="flex items-center gap-3">
                          <span className={`inline-block h-2.5 w-2.5 rounded-full ${cfg.dot}`} aria-hidden="true" />
                          <div>
                            <h3 className="text-sm font-medium text-foreground">{conn.name}</h3>
                            <p className="text-xs text-muted-foreground">
                              {conn.lastSync ? `Last sync: ${conn.lastSync}` : 'Never synced'}
                              {conn.signalCount > 0 && ` \u00B7 ${conn.signalCount} signals`}
                            </p>
                          </div>
                        </div>
                        <div className="flex items-center gap-2">
                          <Badge variant="outline" className={`text-xs ${cfg.color}`}>
                            {conn.status}
                          </Badge>
                          <Button variant="ghost" size="sm">Configure</Button>
                          <Button
                            variant="ghost"
                            size="sm"
                            className="text-red-400 hover:text-red-300"
                            onClick={() => handleDeleteConnector(conn.id)}
                          >
                            Remove
                          </Button>
                        </div>
                      </div>
                    </CardContent>
                  </Card>
                );
              })}
            </div>
          )}
        </TabsContent>

        <TabsContent value="signals" className="flex-1">
          <Card>
            <CardContent className="py-12 text-center">
              <div className="text-4xl mb-3 opacity-50">{'\uD83D\uDCE1'}</div>
              <p className="text-lg font-medium text-muted-foreground">No signals detected</p>
              <p className="text-sm text-muted-foreground/70 mt-1">
                Signals appear here as connectors discover workflow patterns.
              </p>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  );
}
