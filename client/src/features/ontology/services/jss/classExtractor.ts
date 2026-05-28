/**
 * classExtractor — OWL class derivation from a JSON-LD graph.
 *
 * Produces OntologyHierarchy and OntologyMetrics from raw JSON-LD,
 * with no network or cache dependencies.
 */

import { OntologyHierarchy, ClassNode, OntologyMetrics } from '../../store/useOntologyStore';
import { JsonLdNode, JsonLdOntology } from './schemaParser';

// --- OWL class helpers ---

function isOwlClass(node: JsonLdNode): boolean {
  const type = node['@type'];
  if (!type) return false;
  const types = Array.isArray(type) ? type : [type];
  return types.some(
    (t) =>
      t === 'owl:Class' ||
      t === 'http://www.w3.org/2002/07/owl#Class' ||
      t === 'rdfs:Class' ||
      t === 'http://www.w3.org/2000/01/rdf-schema#Class'
  );
}

function extractLabel(node: JsonLdNode): string {
  const label = node['rdfs:label'];
  if (!label) {
    const id = node['@id'];
    const hashIndex = id.lastIndexOf('#');
    const slashIndex = id.lastIndexOf('/');
    const index = Math.max(hashIndex, slashIndex);
    return index >= 0 ? id.slice(index + 1) : id;
  }
  if (typeof label === 'string') return label;
  if (typeof label === 'object' && '@value' in label) return label['@value'];
  return node['@id'];
}

function extractParentId(node: JsonLdNode): string | undefined {
  const subClassOf = node['rdfs:subClassOf'];
  if (!subClassOf) return undefined;

  if (Array.isArray(subClassOf)) {
    for (const parent of subClassOf) {
      if (typeof parent === 'object' && '@id' in parent) {
        const id = parent['@id'];
        if (!id.startsWith('_:')) return id;
      }
    }
    return undefined;
  }

  if (typeof subClassOf === 'object' && '@id' in subClassOf) {
    const id = subClassOf['@id'];
    if (!id.startsWith('_:')) return id;
  }

  return undefined;
}

function getTypes(node: JsonLdNode): string[] {
  const type = node['@type'];
  if (!type) return [];
  return Array.isArray(type) ? type : [type];
}

// --- Public exports ---

export function buildHierarchyFromJsonLd(jsonLd: JsonLdOntology): OntologyHierarchy {
  const classes = new Map<string, ClassNode>();
  const roots: string[] = [];
  const childMap = new Map<string, string[]>();

  const graph = jsonLd['@graph'] || [];

  for (const node of graph) {
    if (!isOwlClass(node)) continue;

    const id = node['@id'];
    const label = extractLabel(node);
    const parentId = extractParentId(node);

    classes.set(id, {
      id,
      label,
      parentId,
      level: 0,
      depth: 0,
      childIds: [],
      instanceCount: 0,
    });

    if (parentId) {
      if (!childMap.has(parentId)) childMap.set(parentId, []);
      childMap.get(parentId)!.push(id);
    }
  }

  for (const [id, node] of classes) {
    const childIds = childMap.get(id) || [];
    node.childIds = childIds;
    node.childIris = childIds;

    if (!node.parentId || !classes.has(node.parentId)) {
      roots.push(id);
    }
  }

  const computeLevel = (id: string, level: number): void => {
    const node = classes.get(id);
    if (node) {
      node.level = level;
      node.depth = level;
      for (const childId of node.childIds || []) {
        computeLevel(childId, level + 1);
      }
    }
  };

  for (const rootId of roots) {
    computeLevel(rootId, 0);
  }

  return { classes, roots };
}

export function extractMetricsFromJsonLd(
  jsonLd: JsonLdOntology,
  fetchCount: number,
  cacheHitCount: number,
  lastFetchDurationMs: number
): OntologyMetrics {
  const graph = jsonLd['@graph'] || [];

  let classCount = 0;
  let propertyCount = 0;
  let individualCount = 0;
  const constraintsByType: Record<string, number> = {};

  for (const node of graph) {
    const types = getTypes(node);

    if (types.includes('owl:Class') || types.includes('rdfs:Class')) classCount++;
    if (
      types.includes('owl:ObjectProperty') ||
      types.includes('owl:DatatypeProperty') ||
      types.includes('rdf:Property')
    ) propertyCount++;
    if (types.includes('owl:NamedIndividual')) individualCount++;

    if (node['owl:disjointWith'])
      constraintsByType['disjointness'] = (constraintsByType['disjointness'] || 0) + 1;
    if (node['rdfs:subClassOf'])
      constraintsByType['subsumption'] = (constraintsByType['subsumption'] || 0) + 1;
    if (node['rdfs:domain'])
      constraintsByType['property_domain'] = (constraintsByType['property_domain'] || 0) + 1;
    if (node['rdfs:range'])
      constraintsByType['property_range'] = (constraintsByType['property_range'] || 0) + 1;
  }

  return {
    axiomCount: graph.length,
    classCount,
    propertyCount,
    individualCount,
    constraintsByType,
    cacheHitRate:
      fetchCount + cacheHitCount > 0
        ? cacheHitCount / (fetchCount + cacheHitCount)
        : 0,
    validationTimeMs: lastFetchDurationMs,
    lastValidated: Date.now(),
  };
}
