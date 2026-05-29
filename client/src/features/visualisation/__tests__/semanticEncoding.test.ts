/**
 * semanticEncoding — unit tests for the pure mapping from agent/memory activity
 * to visual form. Locks the contract the two transient render layers depend on:
 *   - every known memory verb resolves to a distinct, frozen burst profile
 *   - unknown / missing verbs fall back to `access` (never throw, never undefined)
 *   - namespace hue jitter is deterministic, bounded, and zero for empty input
 *   - the burst colour is action-driven and namespace-nudged
 *   - every AgentActionType has a beam shape; unknown types fall back to a probe
 *   - the beam colour palette is re-exported intact from the wire-format module
 */

import { describe, it, expect } from 'vitest';
import * as THREE from 'three';
import { AgentActionType, AGENT_ACTION_COLORS } from '@/services/BinaryWebSocketProtocol';
import {
  memoryActionProfile,
  namespaceHueShift,
  semanticBurstColor,
  agentActionShape,
  agentActionColorHex,
  type MemoryAction,
} from '../semanticEncoding';

const VERBS: MemoryAction[] = ['store', 'retrieve', 'search', 'list', 'delete', 'access'];

describe('memoryActionProfile', () => {
  it('returns a distinct profile for every known verb', () => {
    const colors = VERBS.map((v) => memoryActionProfile(v).color);
    expect(new Set(colors).size).toBe(VERBS.length);
  });

  it('falls back to the access profile for unknown / missing verbs', () => {
    const access = memoryActionProfile('access');
    expect(memoryActionProfile('bogus')).toEqual(access);
    expect(memoryActionProfile(undefined)).toEqual(access);
    expect(memoryActionProfile(null)).toEqual(access);
    expect(memoryActionProfile('')).toEqual(access);
  });

  it('is case-insensitive on the verb', () => {
    expect(memoryActionProfile('STORE')).toEqual(memoryActionProfile('store'));
  });

  it('delete is the only imploding motion; search is the widest scan', () => {
    expect(memoryActionProfile('delete').motion).toBe('implode');
    expect(memoryActionProfile('store').motion).toBe('expand');
    const search = memoryActionProfile('search');
    const others = VERBS.filter((v) => v !== 'search').map((v) => memoryActionProfile(v).maxScale);
    expect(search.maxScale).toBeGreaterThan(Math.max(...others));
  });
});

describe('namespaceHueShift', () => {
  it('is zero for empty / missing namespace', () => {
    expect(namespaceHueShift(undefined)).toBe(0);
    expect(namespaceHueShift(null)).toBe(0);
    expect(namespaceHueShift('')).toBe(0);
  });

  it('is deterministic and bounded by ±maxShift', () => {
    const a = namespaceHueShift('personal-context');
    const b = namespaceHueShift('personal-context');
    expect(a).toBe(b);
    expect(Math.abs(a)).toBeLessThanOrEqual(0.06);
  });

  it('distinguishes different namespaces', () => {
    expect(namespaceHueShift('patterns')).not.toBe(namespaceHueShift('tasks'));
  });
});

describe('semanticBurstColor', () => {
  it('writes the action base colour into the target and returns it', () => {
    const out = new THREE.Color();
    const ret = semanticBurstColor(out, 'store', undefined);
    expect(ret).toBe(out);
    expect(out.getHex()).toBe(memoryActionProfile('store').color);
  });

  it('namespace nudges the hue away from the un-tinted base', () => {
    const base = new THREE.Color();
    semanticBurstColor(base, 'retrieve', undefined);
    const tinted = new THREE.Color();
    semanticBurstColor(tinted, 'retrieve', 'personal-context');
    expect(tinted.getHex()).not.toBe(base.getHex());
  });
});

describe('agentActionShape / agentActionColorHex', () => {
  it('has a shape for every action type', () => {
    for (const t of Object.values(AgentActionType).filter((v) => typeof v === 'number') as number[]) {
      const s = agentActionShape(t);
      expect(s.radialSegments).toBeGreaterThanOrEqual(3);
    }
  });

  it('Create widens toward the node and Delete narrows toward it', () => {
    const create = agentActionShape(AgentActionType.Create);
    const del = agentActionShape(AgentActionType.Delete);
    expect(create.radiusTop).toBeGreaterThan(create.radiusBottom);
    expect(del.radiusTop).toBeLessThan(del.radiusBottom);
  });

  it('unknown action type falls back to the Query probe shape', () => {
    expect(agentActionShape(999)).toEqual(agentActionShape(AgentActionType.Query));
  });

  it('beam colour is re-exported intact from the wire-format palette', () => {
    expect(agentActionColorHex(AgentActionType.Create)).toBe(AGENT_ACTION_COLORS[AgentActionType.Create]);
    expect(agentActionColorHex(999)).toBe('#ffffff');
  });
});
