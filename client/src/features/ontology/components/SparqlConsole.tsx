/**
 * SPARQL Console — read-only SELECT executed SERVER-SIDE over Oxigraph.
 *
 * BINDING CONSTRAINT (PRD-018): no client-side query engine. This component
 * only POSTs the query string to `/api/ontology/sparql` (see sparqlService) and
 * renders the returned SPARQL 1.1 JSON results as a table. Non-200 is surfaced
 * gracefully; the SELECT-only guard runs client-side as UX, server is canonical.
 */

import React, { useState, useCallback } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/features/design-system/components/Card';
import { Button } from '@/features/design-system/components/Button';
import { Badge } from '@/features/design-system/components/Badge';
import { ScrollArea } from '@/features/design-system/components/ScrollArea';
import { Database, Play, AlertCircle, Loader2 } from 'lucide-react';
import {
  runSparqlSelect,
  isReadOnlySelect,
  type SparqlQueryOutcome,
} from '../services/sparqlService';

const DEFAULT_QUERY = `SELECT ?s ?p ?o
WHERE { ?s ?p ?o }
LIMIT 25`;

interface SparqlConsoleProps {
  className?: string;
}

export function SparqlConsole({ className }: SparqlConsoleProps) {
  const [query, setQuery] = useState<string>(DEFAULT_QUERY);
  const [outcome, setOutcome] = useState<SparqlQueryOutcome | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const readOnly = isReadOnlySelect(query);

  const handleRun = useCallback(async () => {
    setError(null);
    setLoading(true);
    try {
      const result = await runSparqlSelect(query);
      setOutcome(result);
    } catch (err: any) {
      setOutcome(null);
      setError(err?.message || 'Query failed');
    } finally {
      setLoading(false);
    }
  }, [query]);

  return (
    <Card className={className} role="region" aria-label="SPARQL query console">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Database className="h-5 w-5" />
          SPARQL Console
        </CardTitle>
        <CardDescription>
          Read-only SELECT over the server-side Oxigraph store. Queries execute on the server; results render here.
        </CardDescription>
      </CardHeader>

      <CardContent className="space-y-4">
        <div className="space-y-2">
          <label htmlFor="sparql-query" className="text-sm font-medium">
            Query
          </label>
          <textarea
            id="sparql-query"
            aria-label="SPARQL SELECT query"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            spellCheck={false}
            rows={6}
            className="w-full rounded-md border bg-muted/40 px-3 py-2 font-mono text-xs focus:outline-none focus:ring-2 focus:ring-primary"
          />
          <div className="flex items-center justify-between">
            <Badge variant={readOnly ? 'secondary' : 'destructive'}>
              {readOnly ? 'Read-only SELECT' : 'Only SELECT/ASK allowed'}
            </Badge>
            <Button onClick={handleRun} disabled={loading || !readOnly} size="sm">
              {loading ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Play className="mr-2 h-4 w-4" />
              )}
              Run
            </Button>
          </div>
        </div>

        {error && (
          <div
            role="alert"
            className="rounded-lg border border-destructive bg-destructive/10 p-3 flex items-start gap-2"
          >
            <AlertCircle className="h-4 w-4 text-destructive mt-0.5" />
            <p className="text-sm text-destructive/90">{error}</p>
          </div>
        )}

        {outcome && (
          <div className="space-y-2">
            <div className="text-sm text-muted-foreground">
              {outcome.rowCount} row{outcome.rowCount === 1 ? '' : 's'}
            </div>
            <ScrollArea className="h-[300px] rounded-md border">
              <table className="w-full text-left text-xs" aria-label="SPARQL results">
                <thead className="sticky top-0 bg-muted/80 backdrop-blur">
                  <tr>
                    {outcome.vars.map((v) => (
                      <th key={v} scope="col" className="px-3 py-2 font-medium">
                        {v}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {outcome.rows.map((row, ri) => (
                    <tr key={ri} className="border-t">
                      {outcome.vars.map((v) => (
                        <td key={v} className="px-3 py-1.5 align-top break-all">
                          <code>{row[v]?.value ?? ''}</code>
                        </td>
                      ))}
                    </tr>
                  ))}
                  {outcome.rows.length === 0 && (
                    <tr>
                      <td
                        colSpan={Math.max(outcome.vars.length, 1)}
                        className="px-3 py-6 text-center text-muted-foreground"
                      >
                        No results
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </ScrollArea>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

export default SparqlConsole;
