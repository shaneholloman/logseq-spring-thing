import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';

// --- Mock external dependencies before importing the store ---

vi.mock('../../../../utils/loggerConfig', () => ({
  createLogger: () => ({
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}));

import { useOntologyStore } from '../useOntologyStore';

describe('useOntologyStore', () => {
  beforeEach(() => {
    // Reset the zustand store state between tests
    useOntologyStore.setState({
      loaded: false,
      validating: false,
      violations: [],
      constraintGroups: [
        { id: 'subsumption', name: 'Subsumption', enabled: true, strength: 0.8, description: 'Class hierarchy constraints', constraintCount: 0, icon: 'hierarchy' },
        { id: 'disjointness', name: 'Disjointness', enabled: true, strength: 1.0, description: 'Disjoint class constraints', constraintCount: 0, icon: 'split' },
        { id: 'property_domain', name: 'Property Domain', enabled: true, strength: 0.9, description: 'Property domain restrictions', constraintCount: 0, icon: 'arrow-right' },
        { id: 'property_range', name: 'Property Range', enabled: true, strength: 0.9, description: 'Property range restrictions', constraintCount: 0, icon: 'arrow-left' },
        { id: 'cardinality', name: 'Cardinality', enabled: false, strength: 0.7, description: 'Property cardinality constraints', constraintCount: 0, icon: 'hash' },
      ],
      metrics: {
        axiomCount: 0,
        classCount: 0,
        propertyCount: 0,
        individualCount: 0,
        constraintsByType: {},
        cacheHitRate: 0,
        validationTimeMs: 0,
      },
      hierarchy: null,
      semanticZoomLevel: 0,
      expandedClasses: new Set<string>(),
      highlightedClass: null,
    });
    vi.clearAllMocks();
    vi.restoreAllMocks();
  });

  // ---- Default state ----

  describe('initial state', () => {
    it('should start with loaded=false and validating=false', () => {
      const state = useOntologyStore.getState();
      expect(state.loaded).toBe(false);
      expect(state.validating).toBe(false);
    });

    it('should have 5 default constraint groups', () => {
      const state = useOntologyStore.getState();
      expect(state.constraintGroups).toHaveLength(5);
    });

    it('should have cardinality disabled by default', () => {
      const cardinality = useOntologyStore.getState().constraintGroups
        .find((g) => g.id === 'cardinality');
      expect(cardinality).toBeDefined();
      expect(cardinality!.enabled).toBe(false);
    });

    it('should have empty violations', () => {
      expect(useOntologyStore.getState().violations).toEqual([]);
    });

    it('should have zeroed metrics', () => {
      const metrics = useOntologyStore.getState().metrics;
      expect(metrics.axiomCount).toBe(0);
      expect(metrics.classCount).toBe(0);
      expect(metrics.propertyCount).toBe(0);
    });

    it('should have no hierarchy initially', () => {
      expect(useOntologyStore.getState().hierarchy).toBeNull();
    });
  });

  // ---- Simple setters ----

  describe('setters', () => {
    it('setLoaded should update loaded state', () => {
      useOntologyStore.getState().setLoaded(true);
      expect(useOntologyStore.getState().loaded).toBe(true);
    });

    it('setValidating should update validating state', () => {
      useOntologyStore.getState().setValidating(true);
      expect(useOntologyStore.getState().validating).toBe(true);
    });

    it('setViolations should replace violations array', () => {
      const violations = [
        { axiomType: 'SubClassOf', description: 'Test violation', severity: 'error' as const, affectedEntities: ['A', 'B'] },
      ];
      useOntologyStore.getState().setViolations(violations);
      expect(useOntologyStore.getState().violations).toEqual(violations);
    });

    it('setMetrics should replace metrics object', () => {
      const metrics = {
        axiomCount: 42,
        classCount: 10,
        propertyCount: 5,
        individualCount: 3,
        constraintsByType: { SubClassOf: 12 },
        cacheHitRate: 0.95,
        validationTimeMs: 120,
      };
      useOntologyStore.getState().setMetrics(metrics);
      expect(useOntologyStore.getState().metrics).toEqual(metrics);
    });
  });

  // ---- Constraint group toggling ----

  describe('toggleConstraintGroup', () => {
    it('should toggle enabled state of a constraint group', () => {
      // Subsumption starts as enabled
      expect(useOntologyStore.getState().constraintGroups[0].enabled).toBe(true);

      useOntologyStore.getState().toggleConstraintGroup('subsumption');
      expect(useOntologyStore.getState().constraintGroups
        .find((g) => g.id === 'subsumption')!.enabled).toBe(false);
    });

    it('should toggle back on second call', () => {
      useOntologyStore.getState().toggleConstraintGroup('subsumption');
      useOntologyStore.getState().toggleConstraintGroup('subsumption');
      expect(useOntologyStore.getState().constraintGroups
        .find((g) => g.id === 'subsumption')!.enabled).toBe(true);
    });

    it('should not affect other constraint groups', () => {
      useOntologyStore.getState().toggleConstraintGroup('subsumption');
      const disjointness = useOntologyStore.getState().constraintGroups
        .find((g) => g.id === 'disjointness');
      expect(disjointness!.enabled).toBe(true); // unchanged
    });

    it('should be a no-op for unknown constraint id', () => {
      const before = [...useOntologyStore.getState().constraintGroups];
      useOntologyStore.getState().toggleConstraintGroup('nonexistent');
      const after = useOntologyStore.getState().constraintGroups;
      expect(after.map((g) => g.enabled)).toEqual(before.map((g) => g.enabled));
    });
  });

  // ---- Strength update ----

  describe('updateStrength', () => {
    it('should update strength of a specific constraint group', () => {
      useOntologyStore.getState().updateStrength('subsumption', 0.5);
      const group = useOntologyStore.getState().constraintGroups
        .find((g) => g.id === 'subsumption');
      expect(group!.strength).toBe(0.5);
    });

    it('should accept boundary values (0 and 1)', () => {
      useOntologyStore.getState().updateStrength('cardinality', 0);
      expect(useOntologyStore.getState().constraintGroups
        .find((g) => g.id === 'cardinality')!.strength).toBe(0);

      useOntologyStore.getState().updateStrength('cardinality', 1);
      expect(useOntologyStore.getState().constraintGroups
        .find((g) => g.id === 'cardinality')!.strength).toBe(1);
    });
  });

  // ---- Hierarchical navigation ----

  describe('toggleClass', () => {
    it('should add class to expanded set', () => {
      useOntologyStore.getState().toggleClass('owl:Thing');
      expect(useOntologyStore.getState().expandedClasses.has('owl:Thing')).toBe(true);
    });

    it('should remove class on second toggle', () => {
      useOntologyStore.getState().toggleClass('owl:Thing');
      useOntologyStore.getState().toggleClass('owl:Thing');
      expect(useOntologyStore.getState().expandedClasses.has('owl:Thing')).toBe(false);
    });

    it('should handle multiple expanded classes', () => {
      useOntologyStore.getState().toggleClass('owl:Thing');
      useOntologyStore.getState().toggleClass('rdfs:Resource');
      const expanded = useOntologyStore.getState().expandedClasses;
      expect(expanded.size).toBe(2);
      expect(expanded.has('owl:Thing')).toBe(true);
      expect(expanded.has('rdfs:Resource')).toBe(true);
    });
  });

  describe('setSemanticZoomLevel', () => {
    it('should update semantic zoom level', () => {
      useOntologyStore.getState().setSemanticZoomLevel(3);
      expect(useOntologyStore.getState().semanticZoomLevel).toBe(3);
    });
  });

  describe('setHighlightedClass', () => {
    it('should set a highlighted class', () => {
      useOntologyStore.getState().setHighlightedClass('owl:Thing');
      expect(useOntologyStore.getState().highlightedClass).toBe('owl:Thing');
    });

    it('should clear highlighted class when set to null', () => {
      useOntologyStore.getState().setHighlightedClass('owl:Thing');
      useOntologyStore.getState().setHighlightedClass(null);
      expect(useOntologyStore.getState().highlightedClass).toBeNull();
    });
  });

  describe('setHierarchy', () => {
    it('should store a hierarchy object', () => {
      const hierarchy = {
        classes: new Map([['owl:Thing', { id: 'owl:Thing', label: 'Thing', level: 0, depth: 0 }]]),
        roots: ['owl:Thing'],
      };
      useOntologyStore.getState().setHierarchy(hierarchy);
      expect(useOntologyStore.getState().hierarchy).toBe(hierarchy);
    });

    it('should clear hierarchy when set to null', () => {
      useOntologyStore.getState().setHierarchy(null);
      expect(useOntologyStore.getState().hierarchy).toBeNull();
    });
  });

  // ---- loadOntology ----

  describe('loadOntology', () => {
    it('should set loaded and metrics on successful fetch', async () => {
      const mockResponse = {
        ok: true,
        json: () => Promise.resolve({
          metrics: { axiomCount: 100, classCount: 20, propertyCount: 5, individualCount: 3, constraintsByType: {}, cacheHitRate: 0.9, validationTimeMs: 50 },
        }),
      };
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(mockResponse as Response);

      await useOntologyStore.getState().loadOntology('http://example.org/ontology.owl');

      const state = useOntologyStore.getState();
      expect(state.loaded).toBe(true);
      expect(state.validating).toBe(false);
      expect(state.metrics.axiomCount).toBe(100);
    });

    it('should throw and remain not loaded on HTTP error', async () => {
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: false,
        statusText: 'Not Found',
      } as Response);

      await expect(
        useOntologyStore.getState().loadOntology('http://example.org/missing.owl'),
      ).rejects.toThrow('Failed to load ontology');

      expect(useOntologyStore.getState().loaded).toBe(false);
      expect(useOntologyStore.getState().validating).toBe(false);
    });

    it('should throw on network error', async () => {
      vi.spyOn(globalThis, 'fetch').mockRejectedValueOnce(new Error('Network error'));

      await expect(
        useOntologyStore.getState().loadOntology('http://example.org/ontology.owl'),
      ).rejects.toThrow('Network error');

      expect(useOntologyStore.getState().validating).toBe(false);
    });

    it('should send POST with correct body', async () => {
      const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({}),
      } as Response);

      await useOntologyStore.getState().loadOntology('http://example.org/test.owl');

      expect(fetchSpy).toHaveBeenCalledWith('/api/ontology/load', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url: 'http://example.org/test.owl' }),
      });
    });
  });

  // ---- validateOntology ----

  describe('validateOntology', () => {
    it('should set violations on successful validation', async () => {
      const violations = [
        { axiomType: 'DisjointClasses', description: 'Overlap found', severity: 'error' as const, affectedEntities: ['A', 'B'] },
      ];
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ violations, metrics: { validationTimeMs: 42 } }),
      } as Response);

      await useOntologyStore.getState().validateOntology();

      const state = useOntologyStore.getState();
      expect(state.violations).toEqual(violations);
      expect(state.validating).toBe(false);
      expect(state.metrics.validationTimeMs).toBe(42);
      expect(state.metrics.lastValidated).toBeDefined();
    });

    it('should only send enabled constraint groups', async () => {
      // Disable all except subsumption
      const state = useOntologyStore.getState();
      state.constraintGroups.forEach((g) => {
        if (g.id !== 'subsumption') {
          useOntologyStore.getState().toggleConstraintGroup(g.id);
        }
      });
      // Cardinality was already disabled; subsumption should remain enabled

      const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ violations: [], metrics: {} }),
      } as Response);

      await useOntologyStore.getState().validateOntology();

      const body = JSON.parse(fetchSpy.mock.calls[0][1]!.body as string);
      const sentIds = body.constraintGroups.map((g: any) => g.id);
      expect(sentIds).toContain('subsumption');
      expect(sentIds).not.toContain('disjointness');
    });

    it('should throw on HTTP error', async () => {
      vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
        ok: false,
        statusText: 'Internal Server Error',
      } as Response);

      await expect(
        useOntologyStore.getState().validateOntology(),
      ).rejects.toThrow('Validation failed');

      expect(useOntologyStore.getState().validating).toBe(false);
    });
  });
});
