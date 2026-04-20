

export type GraphType = 'logseq' | 'visionflow';

/**
 * Node visibility modes (ADR-049 + ADR-051).
 *
 * - `public`   - rendered normally with full label + metadata.
 * - `private`  - opaque-id only, grey/60%-opacity render, no label for non-owners.
 * - `tombstone` - node was unpublished; a short-lived red-X marker is rendered.
 */
export type NodeVisibility = 'public' | 'private' | 'tombstone';

/**
 * Confidence banding for migration candidates.
 * Backend emits a float in [0,1]; we bucket for the 8-signal radar.
 */
export type ConfidenceBand = 'low' | 'medium' | 'high';

export interface KGNode {
  id: string;
  label: string;
  position: {
    x: number;
    y: number;
    z: number;
  };
  metadata?: Record<string, unknown>;
  graphType?: GraphType;
  owlClassIri?: string;  // Ontology class IRI for semantic identity
  nodeType?: string;     // Visual node type for rendering

  // --- Sovereign-mesh extensions (ADR-049 / ADR-051) ---
  /** Current visibility mode for this node. */
  visibility?: NodeVisibility;
  /** Nostr pubkey (hex) of the node owner, if known. */
  owner_pubkey?: string;
  /** Stable opaque identifier surfaced to non-owners for private nodes. */
  opaque_id?: string;
  /** Solid pod URL for the canonical record, when published. */
  pod_url?: string;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  label?: string;
  weight?: number;
  metadata?: Record<string, any>;
  graphType?: GraphType; 
}

export interface TypedGraphData {
  nodes: KGNode[];
  edges: GraphEdge[];
  graphType: GraphType;
  lastUpdate?: number;
}

// Message types for graph updates
export interface GraphUpdateMessage {
  type: 'node-update' | 'edge-update' | 'position-update' | 'bulk-update';
  graphType: GraphType;
  data: any;
  timestamp: number;
}

// Physics settings per graph type
export interface GraphPhysicsConfig {
  
  spring_k: number; 
  repel_k: number; 
  max_velocity: number; 
  damping: number; 
  
  
  rest_length: number; 
  repulsion_cutoff: number; 
  repulsion_softening_epsilon: number; 
  center_gravity_k: number; 
  grid_cell_size: number; 
  warmup_iterations: number; 
  cooling_rate: number; 
  feature_flags: number; 
  
  
  boundary_extreme_multiplier: number; 
  boundary_extreme_force_multiplier: number; 
  boundary_velocity_damping: number; 
  max_force: number; 
  seed: number; 
  iteration: number; 
  
  
  springStrength?: number;
  updateThreshold?: number;
  nodeRepulsion?: number;
  linkDistance?: number;
  gravityStrength?: number;
}

export interface GraphTypeConfig {
  logseq: {
    physics: GraphPhysicsConfig;
    rendering: {
      nodeSize: number;
      edgeWidth: number;
      labelSize: number;
    };
  };
  visionflow: {
    physics: GraphPhysicsConfig;
    rendering: {
      agentSize: number;
      connectionWidth: number;
      healthIndicator: boolean;
    };
  };
}

// =============================================================================
// Sovereign-mesh Sprint 3 types (ADR-048 / ADR-049 / ADR-051)
// =============================================================================

/**
 * 8-signal radar readings surfaced by the Judgment Broker.
 * Each value is a normalised [0,1] score. Missing signals are rendered as 0
 * in the radar component.
 */
export interface MigrationCandidateSignals {
  structural_fit: number;
  semantic_similarity: number;
  provenance_strength: number;
  temporal_stability: number;
  editor_consensus: number;
  reasoner_support: number;
  kg_popularity: number;
  owl_coverage: number;
}

/**
 * A candidate surfaced by the Judgment Broker for promotion from the
 * Knowledge Graph (KG) to an OWL ontology class.
 *
 * Backend shape matches `GET /api/bridge/candidates`.
 */
export interface MigrationCandidate {
  /** Broker-assigned candidate id. */
  id: string;
  /** KG node the broker wants to promote. */
  kg_node: KGNode;
  /** Proposed ontology class metadata (IRI + label + definition). */
  proposed_ontology_class: {
    iri: string;
    label: string;
    definition?: string;
  };
  /** Confidence score [0,1] aggregated from the 8 signals. */
  confidence: number;
  /** Bucketed band for quick visual triage. */
  confidence_band: ConfidenceBand;
  /** Per-signal radar values. */
  signals: MigrationCandidateSignals;
  /** Free-text rationale from the broker. */
  rationale?: string;
  /** Lifecycle status. */
  status: 'surfaced' | 'approved' | 'rejected' | 'deferred';
  /** ISO8601 timestamp when the candidate was surfaced. */
  surfaced_at: string;
}

/**
 * Event emitted when a bridge promotion lands in the graph
 * (Nostr kind 30100 / migration event stream).
 */
export interface BridgePromotionEvent {
  /** Event id (Nostr event hash or server-assigned uuid). */
  id: string;
  /** KG node id the bridge promotes from. */
  from_kg: string;
  /** OWL class IRI the bridge promotes to. */
  to_owl: string;
  /** Final confidence at promotion time. */
  confidence: number;
  /** Edge id created in the graph (if the server minted one). */
  edge_id?: string;
  /** ISO8601 timestamp. */
  at: string;
  /** Optional human-readable summary. */
  summary?: string;
}

/**
 * Visibility transition event emitted when a node flips public <-> private
 * or is tombstoned (ADR-049).
 */
export interface VisibilityTransition {
  /** Event id. */
  id: string;
  /** Target node id in the graph. */
  node_id: string;
  /** Previous visibility (if known). */
  from: NodeVisibility | null;
  /** New visibility after the transition. */
  to: NodeVisibility;
  /** Owner pubkey at the time of the transition. */
  owner_pubkey?: string;
  /** Pod URL if the node just became public. */
  pod_url?: string;
  /** ISO8601 timestamp. */
  at: string;
}

// Default configurations
export const DEFAULT_GRAPH_CONFIG: GraphTypeConfig = {
  logseq: {
    physics: {
      
      spring_k: 0.2,
      repel_k: 1.0,
      max_velocity: 5.0,
      damping: 0.95,
      
      
      rest_length: 50.0,
      repulsion_cutoff: 50.0,
      repulsion_softening_epsilon: 0.0001,
      center_gravity_k: 0.0,
      grid_cell_size: 50.0,
      warmup_iterations: 100,
      cooling_rate: 0.001,
      feature_flags: 7,
      
      
      boundary_extreme_multiplier: 2.0,
      boundary_extreme_force_multiplier: 5.0,
      boundary_velocity_damping: 0.5,
      max_force: 100,
      seed: 42,
      iteration: 0,
      
      
      springStrength: 0.2,
      updateThreshold: 0.05,
      nodeRepulsion: 10,
      linkDistance: 30
    },
    rendering: {
      nodeSize: 5,
      edgeWidth: 1,
      labelSize: 1.2
    }
  },
  visionflow: {
    physics: {
      
      spring_k: 0.3,
      repel_k: 2.0,
      max_velocity: 10.0,
      damping: 0.95,
      
      
      rest_length: 50.0,
      repulsion_cutoff: 50.0,
      repulsion_softening_epsilon: 0.0001,
      center_gravity_k: 0.1,
      grid_cell_size: 50.0,
      warmup_iterations: 100,
      cooling_rate: 0.001,
      feature_flags: 7,
      
      
      boundary_extreme_multiplier: 2.5,
      boundary_extreme_force_multiplier: 6.0,
      boundary_velocity_damping: 0.6,
      max_force: 120,
      seed: 42,
      iteration: 0,
      
      
      springStrength: 0.3,
      updateThreshold: 0.1,
      nodeRepulsion: 15,
      linkDistance: 20,
      gravityStrength: 0.1
    },
    rendering: {
      agentSize: 8,
      connectionWidth: 2,
      healthIndicator: true
    }
  }
};