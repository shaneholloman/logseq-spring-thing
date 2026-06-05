// Components
export { OntologyPanel } from './components/OntologyPanel';
export { ConstraintGroupControl } from './components/ConstraintGroupControl';
export { ValidationStatus } from './components/ValidationStatus';
export { OntologyMetrics } from './components/OntologyMetrics';
export { InferencePanel } from './components/InferencePanel';
export { OntologyContribution } from './components/OntologyContribution';
export { OntologyProposalList } from './components/OntologyProposalList';
export { OntologyBrowser } from './components/OntologyBrowser';
export { OntologyTabContent } from './components/OntologyTabContent';
export { SparqlConsole } from './components/SparqlConsole';
export { OntologyExplorationControls } from './components/OntologyExplorationControls';

// Stores
export { useInferredEdgesStore } from './store/useInferredEdgesStore';

// Services - SPARQL + inferred axioms
export { runSparqlSelect, isReadOnlySelect, SPARQL_ENDPOINT } from './services/sparqlService';
export type { SparqlQueryOutcome, SparqlSelectResult, SparqlTerm } from './services/sparqlService';
export {
  fetchReasoningReport,
  INFERRED_NAMED_GRAPH,
  INFERRED_ENDPOINT,
  EMPTY_REASONING_REPORT,
} from './services/inferredAxiomsService';
export type { ReasoningReport, InferredTriple } from './services/inferredAxiomsService';

// Store - types and hook
export { useOntologyStore } from './store/useOntologyStore';
export type {
  OntologyState,
  OntologyMetrics as OntologyMetricsType,
  ClassNode,
  OntologyHierarchy,
  Violation,
  ConstraintGroup
} from './store/useOntologyStore';

// Hooks
export * from './hooks/useOntologyWebSocket';
export * from './hooks/useOntologyStore';

// Services
export { jssOntologyService } from './services/JssOntologyService';
export type {
  JsonLdContext,
  JsonLdOntology,
  JsonLdNode,
  OntologyChangeEvent,
  OntologyChangeCallback,
  FetchOptions,
} from './services/JssOntologyService';
