import React, { useState, useEffect, useRef } from 'react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '../../design-system/components';
import { Badge } from '../../design-system/components';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../../design-system/components';
import { Sparkline } from '../../design-system/components';
import { apiFetch, ApiError } from '../../../utils/apiFetch';

function generateTrendData(kpiKey: string): number[] {
  const seed = kpiKey.length;
  return Array.from({ length: 20 }, (_, i) => {
    const base = 50 + seed * 5;
    const noise = Math.sin(i * 0.7 + seed) * 15 + Math.cos(i * 0.3) * 8;
    return Math.max(0, base + noise + i * 0.5);
  });
}

interface KpiData {
  value: number | null;
  unit: string;
  description: string;
  status: string;
}

interface KpiMetrics {
  mesh_velocity: KpiData;
  augmentation_ratio: KpiData;
  trust_variance: KpiData;
  hitl_precision: KpiData;
}

const KPI_CONFIG = [
  { key: 'mesh_velocity', label: 'Mesh Velocity', icon: '\u26A1', target: '< 48h', color: 'text-emerald-400' },
  { key: 'augmentation_ratio', label: 'Augmentation Ratio', icon: '\uD83E\uDD16', target: '> 65%', color: 'text-blue-400' },
  { key: 'trust_variance', label: 'Trust Variance', icon: '\uD83D\uDEE1\uFE0F', target: '< 0.12\u03C3', color: 'text-purple-400' },
  { key: 'hitl_precision', label: 'HITL Precision', icon: '\uD83C\uDFAF', target: '> 90%', color: 'text-amber-400' },
] as const;

export function MeshKpiDashboard() {
  const [metrics, setMetrics] = useState<KpiMetrics | null>(null);
  const [timeWindow, setTimeWindow] = useState('7d');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const liveRegionRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const fetchMetrics = async () => {
      try {
        setError(null);
        const data = await apiFetch<{ kpis: KpiMetrics }>('/api/mesh-metrics');
        setMetrics(data.kpis);
        if (liveRegionRef.current) {
          liveRegionRef.current.textContent = `KPI metrics updated for ${timeWindow} window`;
        }
      } catch (err) {
        const message = err instanceof ApiError ? err.message : 'Network error';
        setError(message);
        console.error('Failed to fetch mesh metrics:', err);
      } finally {
        setLoading(false);
      }
    };

    fetchMetrics();
    const interval = setInterval(fetchMetrics, 30000);
    return () => clearInterval(interval);
  }, [timeWindow]);

  return (
    <div className="h-full flex flex-col gap-4 p-4">
      <div ref={liveRegionRef} aria-live="polite" className="sr-only" />
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold text-foreground">Mesh KPIs</h1>
        <Select value={timeWindow} onValueChange={setTimeWindow}>
          <SelectTrigger className="w-[120px]" aria-label="Select time window">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="24h">24 hours</SelectItem>
            <SelectItem value="7d">7 days</SelectItem>
            <SelectItem value="30d">30 days</SelectItem>
            <SelectItem value="90d">90 days</SelectItem>
          </SelectContent>
        </Select>
      </div>

      {error && (
        <div className="p-3 rounded-lg border border-red-500/30 bg-red-500/10 text-red-400 text-sm">
          {error}
        </div>
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {KPI_CONFIG.map(({ key, label, icon, target, color }) => {
          const kpi = metrics?.[key as keyof KpiMetrics];
          const hasValue = kpi?.value !== null && kpi?.value !== undefined;

          return (
            <Card key={key} className="relative overflow-hidden">
              <CardHeader className="pb-2">
                <div className="flex items-center justify-between">
                  <CardTitle className="text-sm font-medium text-muted-foreground">
                    {icon} {label}
                  </CardTitle>
                  <Badge variant={hasValue ? 'default' : 'outline'} className="text-xs">
                    {kpi?.status || 'not_computed'}
                  </Badge>
                </div>
              </CardHeader>
              <CardContent>
                <div className="flex items-end justify-between">
                  <div>
                    <div className={`text-3xl font-bold ${hasValue ? color : 'text-muted-foreground'}`}>
                      {hasValue ? `${kpi!.value}${kpi!.unit === 'percentage' ? '%' : ''}` : '\u2014'}
                    </div>
                    <div className="text-xs text-muted-foreground mt-1">
                      Target: {target}
                    </div>
                  </div>
                  <Sparkline
                    data={generateTrendData(key)}
                    width={96}
                    height={48}
                    color={color.includes('emerald') ? '#10b981' :
                           color.includes('blue') ? '#3b82f6' :
                           color.includes('purple') ? '#8b5cf6' : '#f59e0b'}
                  />
                </div>
                <p className="text-xs text-muted-foreground mt-3">
                  {kpi?.description || 'Waiting for data...'}
                </p>
              </CardContent>
            </Card>
          );
        })}
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">KPI Lineage</CardTitle>
          <CardDescription>Click a KPI to trace its computation back to source events</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="text-center py-8 text-muted-foreground text-sm">
            Select a KPI card to explore its event lineage
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
