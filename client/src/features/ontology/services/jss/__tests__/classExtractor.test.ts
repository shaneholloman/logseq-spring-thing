import { describe, it, expect } from 'vitest';
import { buildHierarchyFromJsonLd, extractMetricsFromJsonLd } from '../classExtractor';
import type { JsonLdOntology } from '../schemaParser';

function ontology(graph: any[]): JsonLdOntology {
  return { '@context': {}, '@graph': graph };
}

const OWL_CLASS = 'owl:Class';
const OWL_NAMED_INDIVIDUAL = 'owl:NamedIndividual';

describe('buildHierarchyFromJsonLd', () => {
  it('returns empty hierarchy for empty graph', () => {
    const h = buildHierarchyFromJsonLd(ontology([]));
    expect(h.classes.size).toBe(0);
    expect(h.roots).toHaveLength(0);
  });

  it('skips non-OWL-class nodes', () => {
    const h = buildHierarchyFromJsonLd(ontology([
      { '@id': 'ex:prop', '@type': 'owl:ObjectProperty' }
    ]));
    expect(h.classes.size).toBe(0);
  });

  it('creates a root class with no parent', () => {
    const h = buildHierarchyFromJsonLd(ontology([
      { '@id': 'ex:Animal', '@type': OWL_CLASS, 'rdfs:label': 'Animal' }
    ]));
    expect(h.classes.size).toBe(1);
    expect(h.roots).toContain('ex:Animal');
    const node = h.classes.get('ex:Animal')!;
    expect(node.label).toBe('Animal');
    expect(node.level).toBe(0);
    expect(node.parentId).toBeUndefined();
  });

  it('assigns correct parent/child relationship', () => {
    const h = buildHierarchyFromJsonLd(ontology([
      { '@id': 'ex:Animal', '@type': OWL_CLASS },
      {
        '@id': 'ex:Dog',
        '@type': OWL_CLASS,
        'rdfs:subClassOf': { '@id': 'ex:Animal' }
      }
    ]));
    const dog = h.classes.get('ex:Dog')!;
    expect(dog.parentId).toBe('ex:Animal');
    expect(dog.level).toBe(1);
    const animal = h.classes.get('ex:Animal')!;
    expect(animal.childIds).toContain('ex:Dog');
  });

  it('uses @id fragment as label when rdfs:label absent', () => {
    const h = buildHierarchyFromJsonLd(ontology([
      { '@id': 'http://example.org/onto#MyClass', '@type': OWL_CLASS }
    ]));
    const node = h.classes.get('http://example.org/onto#MyClass')!;
    expect(node.label).toBe('MyClass');
  });

  it('skips blank-node parents (starting with _:)', () => {
    const h = buildHierarchyFromJsonLd(ontology([
      { '@id': 'ex:A', '@type': OWL_CLASS },
      { '@id': 'ex:B', '@type': OWL_CLASS, 'rdfs:subClassOf': { '@id': '_:blank1' } }
    ]));
    expect(h.classes.get('ex:B')!.parentId).toBeUndefined();
  });

  it('handles rdfs:Class as well as owl:Class', () => {
    const h = buildHierarchyFromJsonLd(ontology([
      { '@id': 'ex:Thing', '@type': 'rdfs:Class' }
    ]));
    expect(h.classes.size).toBe(1);
  });

  it('computes correct depth for three-level chain', () => {
    const h = buildHierarchyFromJsonLd(ontology([
      { '@id': 'ex:A', '@type': OWL_CLASS },
      { '@id': 'ex:B', '@type': OWL_CLASS, 'rdfs:subClassOf': { '@id': 'ex:A' } },
      { '@id': 'ex:C', '@type': OWL_CLASS, 'rdfs:subClassOf': { '@id': 'ex:B' } },
    ]));
    expect(h.classes.get('ex:A')!.level).toBe(0);
    expect(h.classes.get('ex:B')!.level).toBe(1);
    expect(h.classes.get('ex:C')!.level).toBe(2);
  });
});

describe('extractMetricsFromJsonLd', () => {
  it('returns all-zero metrics for empty graph', () => {
    const m = extractMetricsFromJsonLd(ontology([]), 0, 0, 0);
    expect(m.classCount).toBe(0);
    expect(m.propertyCount).toBe(0);
    expect(m.individualCount).toBe(0);
    expect(m.axiomCount).toBe(0);
  });

  it('counts owl:Class nodes', () => {
    const m = extractMetricsFromJsonLd(ontology([
      { '@id': 'ex:A', '@type': OWL_CLASS },
      { '@id': 'ex:B', '@type': OWL_CLASS },
    ]), 1, 0, 50);
    expect(m.classCount).toBe(2);
  });

  it('counts owl:NamedIndividual nodes', () => {
    const m = extractMetricsFromJsonLd(ontology([
      { '@id': 'ex:fido', '@type': OWL_NAMED_INDIVIDUAL }
    ]), 0, 0, 0);
    expect(m.individualCount).toBe(1);
  });

  it('counts property nodes', () => {
    const m = extractMetricsFromJsonLd(ontology([
      { '@id': 'ex:hasName', '@type': 'owl:DatatypeProperty' }
    ]), 0, 0, 0);
    expect(m.propertyCount).toBe(1);
  });

  it('calculates cache hit rate correctly', () => {
    const m = extractMetricsFromJsonLd(ontology([]), 3, 7, 0);
    // 7 hits out of 10 total = 0.7
    expect(m.cacheHitRate).toBeCloseTo(0.7, 5);
  });

  it('handles zero fetch + zero cache without division by zero', () => {
    const m = extractMetricsFromJsonLd(ontology([]), 0, 0, 0);
    expect(m.cacheHitRate).toBe(0);
  });

  it('counts subsumption constraints from rdfs:subClassOf', () => {
    const m = extractMetricsFromJsonLd(ontology([
      { '@id': 'ex:A', '@type': OWL_CLASS, 'rdfs:subClassOf': { '@id': 'ex:B' } },
    ]), 0, 0, 0);
    expect(m.constraintsByType['subsumption']).toBe(1);
  });

  it('axiomCount equals total graph node count', () => {
    const graph = [
      { '@id': 'ex:A', '@type': OWL_CLASS },
      { '@id': 'ex:B', '@type': OWL_CLASS },
      { '@id': 'ex:p',  '@type': 'owl:ObjectProperty' },
    ];
    const m = extractMetricsFromJsonLd(ontology(graph), 0, 0, 0);
    expect(m.axiomCount).toBe(3);
  });
});
