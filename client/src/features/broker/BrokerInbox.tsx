/**
 * Judgment Broker Inbox - Migration Candidates (ADR-051, Sprint 3).
 *
 * NOTE: this is distinct from `features/broker/components/BrokerInbox.tsx`,
 * which is the case-management inbox for the Policy/Workflow broker. This
 * file implements the bridge-candidate inbox for the KG->OWL migration
 * surface introduced in Sprint 3.
 *
 * Layout (DecisionCanvas):
 *   +-------------------------------------------------------+
 *   | [KG node metadata] [8-signal radar] [proposed OWL]    |
 *   |-------------------------------------------------------|
 *   | [Approve] [Reject] [Defer]                            |
 *   +-------------------------------------------------------+
 *
 * Poll cadence: 30s. Gated by feature flag `BRIDGE_EDGE_ENABLED`.
 */

import React, { useEffect, useMemo, useState } from 'react';
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  EmptyState,
  Textarea,
} from '../design-system/components';
import { useBrokerStore, type BrokerDecision } from '../graph/store/brokerSlice';
import { useFeatureFlag } from '../../services/featureFlags';
import type {
  MigrationCandidate,
  MigrationCandidateSignals,
} from '../graph/types/graphTypes';

const POLL_INTERVAL_MS = 30_000;

const SIGNAL_LABELS: Record<keyof MigrationCandidateSignals, string> = {
  structural_fit: 'Structural',
  semantic_similarity: 'Semantic',
  provenance_strength: 'Provenance',
  temporal_stability: 'Temporal',
  editor_consensus: 'Editors',
  reasoner_support: 'Reasoner',
  kg_popularity: 'KG pop.',
  owl_coverage: 'OWL cov.',
};

const SIGNAL_KEYS: Array<keyof MigrationCandidateSignals> = [
  'structural_fit',
  'semantic_similarity',
  'provenance_strength',
  'temporal_stability',
  'editor_consensus',
  'reasoner_support',
  'kg_popularity',
  'owl_coverage',
];

// =============================================================================
// 8-signal radar (SVG, no chart library)
// =============================================================================

interface RadarProps {
  signals: MigrationCandidateSignals;
  size?: number;
}

function clamp01(n: number): number {
  if (Number.isNaN(n) || !Number.isFinite(n)) return 0;
  if (n < 0) return 0;
  if (n > 1) return 1;
  return n;
}

function Radar({ signals, size = 180 }: RadarProps): React.ReactElement {
  const cx = size / 2;
  const cy = size / 2;
  const r = size / 2 - 16;
  const n = SIGNAL_KEYS.length;

  const points = SIGNAL_KEYS.map((key, i) => {
    const v = clamp01(signals[key] ?? 0);
    const angle = (Math.PI * 2 * i) / n - Math.PI / 2;
    const x = cx + Math.cos(angle) * r * v;
    const y = cy + Math.sin(angle) * r * v;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(' ');

  const axes = SIGNAL_KEYS.map((key, i) => {
    const angle = (Math.PI * 2 * i) / n - Math.PI / 2;
    const x = cx + Math.cos(angle) * r;
    const y = cy + Math.sin(angle) * r;
    return (
      <g key={key}>
        <line
          x1={cx}
          y1={cy}
          x2={x}
          y2={y}
          stroke="currentColor"
          strokeOpacity={0.15}
          strokeWidth={1}
        />
        <text
          x={cx + Math.cos(angle) * (r + 10)}
          y={cy + Math.sin(angle) * (r + 10)}
          fontSize={9}
          textAnchor="middle"
          dominantBaseline="middle"
          fill="currentColor"
          opacity={0.7}
        >
          {SIGNAL_LABELS[key]}
        </text>
      </g>
    );
  });

  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      role="img"
      aria-label="8-signal confidence radar"
      className="text-foreground"
    >
      {[0.25, 0.5, 0.75, 1].map((t) => (
        <circle
          key={t}
          cx={cx}
          cy={cy}
          r={r * t}
          fill="none"
          stroke="currentColor"
          strokeOpacity={0.08}
        />
      ))}
      {axes}
      <polygon
        points={points}
        fill="currentColor"
        fillOpacity={0.2}
        stroke="currentColor"
        strokeWidth={1.5}
        className="text-cyan-400"
      />
    </svg>
  );
}

// =============================================================================
// DecisionCanvas - single-candidate detail + action footer
// =============================================================================

interface DecisionCanvasProps {
  candidate: MigrationCandidate;
  busy: boolean;
  onDecide: (decision: BrokerDecision, reason?: string) => void;
  onClose: () => void;
}

const BAND_TONE: Record<MigrationCandidate['confidence_band'], string> = {
  low: 'bg-red-500/20 text-red-400 border-red-500/30',
  medium: 'bg-amber-500/20 text-amber-400 border-amber-500/30',
  high: 'bg-emerald-500/20 text-emerald-400 border-emerald-500/30',
};

function MigrationDecisionCanvas({
  candidate,
  busy,
  onDecide,
  onClose,
}: DecisionCanvasProps): React.ReactElement {
  const [reason, setReason] = useState('');

  const pct = Math.round(clamp01(candidate.confidence) * 100);
  const kgMeta = useMemo(
    () =>
      Object.entries(candidate.kg_node.metadata ?? {}).filter(
        ([, v]) => v !== null && v !== undefined && v !== '',
      ),
    [candidate.kg_node.metadata],
  );

  return (
    <Card className="border-cyan-500/30">
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between gap-2">
          <CardTitle className="text-base">
            {candidate.kg_node.label}
            <span className="ml-2 text-xs text-muted-foreground">
              {candidate.kg_node.id}
            </span>
          </CardTitle>
          <div className="flex items-center gap-2">
            <span
              className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium border ${BAND_TONE[candidate.confidence_band]}`}
            >
              {candidate.confidence_band} - {pct}%
            </span>
            <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>
              Close
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          {/* Left - KG metadata */}
          <section aria-label="KG node metadata" className="space-y-1">
            <h3 className="text-xs uppercase tracking-wide text-muted-foreground">
              KG node
            </h3>
            <dl className="text-sm space-y-1">
              <div className="flex gap-2">
                <dt className="text-muted-foreground shrink-0">Type:</dt>
                <dd className="truncate">{candidate.kg_node.nodeType ?? '-'}</dd>
              </div>
              {candidate.kg_node.owlClassIri && (
                <div className="flex gap-2">
                  <dt className="text-muted-foreground shrink-0">Class IRI:</dt>
                  <dd className="truncate" title={candidate.kg_node.owlClassIri}>
                    {candidate.kg_node.owlClassIri}
                  </dd>
                </div>
              )}
              {kgMeta.slice(0, 6).map(([k, v]) => (
                <div key={k} className="flex gap-2">
                  <dt className="text-muted-foreground shrink-0">{k}:</dt>
                  <dd className="truncate">{String(v)}</dd>
                </div>
              ))}
            </dl>
          </section>

          {/* Centre - Radar */}
          <section
            aria-label="Confidence signal radar"
            className="flex items-center justify-center"
          >
            <Radar signals={candidate.signals} />
          </section>

          {/* Right - OWL target */}
          <section aria-label="Proposed ontology class" className="space-y-1">
            <h3 className="text-xs uppercase tracking-wide text-muted-foreground">
              Proposed OWL class
            </h3>
            <p className="text-sm font-medium">
              {candidate.proposed_ontology_class.label}
            </p>
            <p
              className="text-xs text-muted-foreground break-all"
              title={candidate.proposed_ontology_class.iri}
            >
              {candidate.proposed_ontology_class.iri}
            </p>
            {candidate.proposed_ontology_class.definition && (
              <p className="text-xs mt-2 text-foreground/80">
                {candidate.proposed_ontology_class.definition}
              </p>
            )}
            {candidate.rationale && (
              <div className="mt-3 p-2 rounded border border-border bg-muted/20">
                <p className="text-xs text-muted-foreground">Broker rationale</p>
                <p className="text-xs mt-1">{candidate.rationale}</p>
              </div>
            )}
          </section>
        </div>

        {/* Reason */}
        <div>
          <label
            htmlFor={`reason-${candidate.id}`}
            className="text-xs text-muted-foreground"
          >
            Reason (optional - stored with the decision)
          </label>
          <Textarea
            id={`reason-${candidate.id}`}
            value={reason}
            onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) =>
              setReason(e.target.value)
            }
            placeholder="Why are you promoting / rejecting / deferring this candidate?"
            rows={2}
            className="mt-1"
          />
        </div>

        {/* Action footer */}
        <div className="flex items-center justify-end gap-2">
          <Button
            variant="outline"
            disabled={busy}
            onClick={() => onDecide('defer', reason)}
          >
            Defer
          </Button>
          <Button
            variant="destructive"
            disabled={busy}
            onClick={() => onDecide('reject', reason)}
          >
            Reject
          </Button>
          <Button
            className="bg-emerald-500/20 text-emerald-300 hover:bg-emerald-500/30 border border-emerald-500/40"
            disabled={busy}
            onClick={() => onDecide('promote', reason)}
          >
            {busy ? 'Working...' : 'Approve'}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

// =============================================================================
// BrokerInbox - panel shell
// =============================================================================

export interface BrokerInboxProps {
  /** Optional override for the poll cadence (milliseconds). */
  pollIntervalMs?: number;
  /** When true, collapse to a floating FAB-style panel. */
  compact?: boolean;
}

export function BrokerInbox({
  pollIntervalMs = POLL_INTERVAL_MS,
  compact = false,
}: BrokerInboxProps): React.ReactElement | null {
  const enabled = useFeatureFlag('BRIDGE_EDGE_ENABLED');
  const candidates = useBrokerStore((s) => s.candidates);
  const selectedId = useBrokerStore((s) => s.selectedId);
  const decidingId = useBrokerStore((s) => s.decidingId);
  const error = useBrokerStore((s) => s.error);
  const loading = useBrokerStore((s) => s.loading);
  const fetchCandidates = useBrokerStore((s) => s.fetchCandidates);
  const select = useBrokerStore((s) => s.select);
  const decide = useBrokerStore((s) => s.decide);
  const clearError = useBrokerStore((s) => s.clearError);

  const [expanded, setExpanded] = useState(!compact);

  useEffect(() => {
    if (!enabled) return;
    void fetchCandidates();
    const handle = window.setInterval(() => {
      void fetchCandidates();
    }, pollIntervalMs);
    return () => window.clearInterval(handle);
  }, [enabled, fetchCandidates, pollIntervalMs]);

  const selected = useMemo(
    () => candidates.find((c) => c.id === selectedId) ?? null,
    [candidates, selectedId],
  );

  if (!enabled) return null;

  return (
    <aside
      aria-label="Judgment Broker inbox"
      className="fixed right-4 top-20 z-40 w-[min(560px,calc(100vw-2rem))] max-h-[80vh] overflow-hidden rounded-lg border border-border bg-background/95 backdrop-blur shadow-2xl flex flex-col"
    >
      <header className="flex items-center justify-between px-3 py-2 border-b border-border">
        <div className="flex items-center gap-2">
          <h2 className="text-sm font-semibold">Broker Inbox</h2>
          {candidates.length > 0 && (
            <Badge variant="destructive">{candidates.length}</Badge>
          )}
          {loading && (
            <span className="text-xs text-muted-foreground">refreshing...</span>
          )}
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setExpanded((e) => !e)}
          aria-expanded={expanded}
          aria-controls="broker-inbox-body"
        >
          {expanded ? 'Minimise' : 'Expand'}
        </Button>
      </header>

      {expanded && (
        <div id="broker-inbox-body" className="flex-1 overflow-auto p-3 space-y-3">
          {error && (
            <div
              role="alert"
              className="p-2 rounded border border-red-500/30 bg-red-500/10 text-red-400 text-xs flex items-center justify-between gap-2"
            >
              <span>{error}</span>
              <Button variant="ghost" size="sm" onClick={clearError}>
                dismiss
              </Button>
            </div>
          )}

          {selected ? (
            <MigrationDecisionCanvas
              candidate={selected}
              busy={decidingId === selected.id}
              onDecide={(decision, reason) =>
                void decide(selected.id, decision, reason)
              }
              onClose={() => select(null)}
            />
          ) : candidates.length === 0 ? (
            <EmptyState
              title="No surfaced candidates"
              description="The Judgment Broker has not surfaced any KG->OWL promotions. Candidates appear here when the 8-signal confidence passes the surface threshold."
            />
          ) : (
            <ul className="space-y-2">
              {candidates.map((c) => (
                <li key={c.id}>
                  <button
                    type="button"
                    onClick={() => select(c.id)}
                    className="w-full text-left p-2 rounded border border-border hover:border-cyan-400/50 hover:bg-muted/40 transition-colors"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-medium text-sm truncate">
                        {c.kg_node.label}
                      </span>
                      <span
                        className={`inline-flex items-center px-1.5 py-0.5 rounded-full text-[10px] font-medium border ${BAND_TONE[c.confidence_band]}`}
                      >
                        {Math.round(clamp01(c.confidence) * 100)}%
                      </span>
                    </div>
                    <p className="text-xs text-muted-foreground truncate mt-0.5">
                      {c.proposed_ontology_class.label}
                    </p>
                    <p className="text-[10px] text-muted-foreground/80 mt-0.5">
                      surfaced {new Date(c.surfaced_at).toLocaleString()}
                    </p>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </aside>
  );
}

export default BrokerInbox;
