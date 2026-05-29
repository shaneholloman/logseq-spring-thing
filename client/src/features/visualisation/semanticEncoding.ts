/**
 * semanticEncoding — the single source of truth for how agent/memory activity
 * is mapped to visual form across the transient render layers.
 *
 * Two activity streams feed the embodiment:
 *   - memory_flash events (RuVector access)  → EmbeddingCloudLayer burst rings
 *   - 0x23 AGENT_ACTION beams (KG mutation)   → TransientBeamsLayer cylinders
 *
 * Before this module each layer encoded its own ad-hoc colours: the cloud picked
 * a *random* burst colour ignoring the `action`/`namespace` it received, and the
 * beam layer reached straight into AGENT_ACTION_COLORS with no shape variation.
 * Centralising the mapping makes the visuals semantic (the colour/shape/motion
 * now *means* something) and keeps the two layers from drifting apart.
 *
 * The beam colour palette (AGENT_ACTION_COLORS) still lives with the wire format
 * in binaryProtocol/frameTypes — this module re-exports and extends it so every
 * visual consumer has one import.
 */

import * as THREE from 'three';
import { AGENT_ACTION_COLORS, AgentActionType } from '@/services/BinaryWebSocketProtocol';

export { AGENT_ACTION_COLORS, AgentActionType };

// ─── Memory-flash bursts (EmbeddingCloudLayer) ──────────────────────────────────

/**
 * Action verbs emitted by the memory_flash producers (ruvector-mcp.cjs,
 * management-api/routes/memory.js, VisionClaw memory_flash_handler.rs). `access`
 * is the catch-all default both producers fall back to.
 */
export type MemoryAction = 'store' | 'retrieve' | 'search' | 'list' | 'delete' | 'access';

export interface BurstProfile {
  /** base colour (0xRRGGBB) before the namespace hue jitter is applied */
  color: number;
  /** world-units radius at peak expansion */
  maxScale: number;
  /** lifetime in seconds */
  duration: number;
  /** 'expand' grows outward (default); 'implode' starts large and contracts */
  motion: 'expand' | 'implode';
  /** concentric ring count — visual weight of the event */
  rings: number;
}

/**
 * Each verb gets a colour that reads as its meaning and a motion that embodies
 * it: writes punch outward green, reads pulse blue, searches ripple wide cyan,
 * deletes implode red. Tuned to stay legible against the cloud's own point hues.
 */
const MEMORY_ACTION_PROFILES: Readonly<Record<MemoryAction, BurstProfile>> = Object.freeze({
  store:    { color: 0x39ff14, maxScale: 4.6, duration: 1.6, motion: 'expand',  rings: 2 }, // electric green — creation
  retrieve: { color: 0x4fc3f7, maxScale: 3.4, duration: 1.8, motion: 'expand',  rings: 1 }, // blue — read
  search:   { color: 0x00fff7, maxScale: 5.8, duration: 2.2, motion: 'expand',  rings: 3 }, // cyan — wide scan
  list:     { color: 0xffd54f, maxScale: 3.0, duration: 1.4, motion: 'expand',  rings: 1 }, // amber — enumerate
  delete:   { color: 0xff4444, maxScale: 4.0, duration: 1.4, motion: 'implode', rings: 1 }, // red — removal
  access:   { color: 0x9ad6ff, maxScale: 3.2, duration: 1.6, motion: 'expand',  rings: 1 }, // neutral default
});

/** Profile for a memory action verb; unknown verbs fall back to `access`. */
export function memoryActionProfile(action: string | undefined | null): BurstProfile {
  const key = (action || 'access').toLowerCase() as MemoryAction;
  return MEMORY_ACTION_PROFILES[key] ?? MEMORY_ACTION_PROFILES.access;
}

/**
 * Deterministic namespace → hue rotation in [-maxShift, +maxShift] turns. Gives
 * each namespace a stable sub-identity tint without overpowering the action
 * colour (small default shift keeps the verb legible).
 */
export function namespaceHueShift(ns: string | undefined | null, maxShift = 0.06): number {
  if (!ns) return 0;
  let h = 0;
  for (let i = 0; i < ns.length; i++) h = (h * 31 + ns.charCodeAt(i)) >>> 0;
  const norm = (h % 1000) / 1000; // 0..1
  return (norm * 2 - 1) * maxShift; // -maxShift..+maxShift
}

/**
 * Write the semantic burst colour for (action, namespace) into `out`: the action
 * sets the base hue, the namespace nudges it for sub-identity. Returns `out`.
 */
export function semanticBurstColor(
  out: THREE.Color,
  action: string | undefined | null,
  ns: string | undefined | null,
): THREE.Color {
  out.setHex(memoryActionProfile(action).color);
  const shift = namespaceHueShift(ns);
  if (shift !== 0) {
    const hsl = { h: 0, s: 0, l: 0 };
    out.getHSL(hsl);
    out.setHSL((hsl.h + shift + 1) % 1, hsl.s, hsl.l);
  }
  return out;
}

// ─── Agent-action beams (TransientBeamsLayer) ───────────────────────────────────

export interface BeamShape {
  /** radius multiplier at the target (+Y) end of the stretched cylinder */
  radiusTop: number;
  /** radius multiplier at the agent (-Y) end */
  radiusBottom: number;
  /** cylinder radial segments — higher reads rounder/smoother */
  radialSegments: number;
}

/**
 * The cylinder is stretched src(agent,-Y) → tgt(node,+Y), so radiusTop is the
 * end touching the KG node. Shapes embody the verb: Create *widens* into the
 * node (deposit), Delete *narrows* into it (withdraw), Query is a thin probe,
 * Link is a thick tie, Transform is rounder.
 */
const AGENT_ACTION_SHAPES: Readonly<Record<AgentActionType, BeamShape>> = Object.freeze({
  [AgentActionType.Query]:     { radiusTop: 0.5, radiusBottom: 0.5, radialSegments: 8 },
  [AgentActionType.Update]:    { radiusTop: 1.0, radiusBottom: 1.0, radialSegments: 10 },
  [AgentActionType.Create]:    { radiusTop: 1.8, radiusBottom: 0.4, radialSegments: 12 },
  [AgentActionType.Delete]:    { radiusTop: 0.3, radiusBottom: 1.6, radialSegments: 10 },
  [AgentActionType.Link]:      { radiusTop: 1.3, radiusBottom: 1.3, radialSegments: 12 },
  [AgentActionType.Transform]: { radiusTop: 0.9, radiusBottom: 0.9, radialSegments: 16 },
});

/** Beam shape for an action type; unknown types fall back to the Query probe. */
export function agentActionShape(actionType: number): BeamShape {
  return AGENT_ACTION_SHAPES[actionType as AgentActionType] ?? AGENT_ACTION_SHAPES[AgentActionType.Query];
}

/** Beam colour for an action type; unknown types fall back to white. */
export function agentActionColorHex(actionType: number): string {
  return AGENT_ACTION_COLORS[actionType as AgentActionType] ?? '#ffffff';
}
