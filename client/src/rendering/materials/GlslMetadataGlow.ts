import * as THREE from 'three';
import { isWebGPURenderer } from '../rendererFactory';
import { createLogger } from '../../utils/loggerConfig';

const logger = createLogger('GlslMetadataGlow');

/**
 * WebGL counterpart to the WebGPU TSL metadata materials (createTslGemMaterial /
 * createTslCrystalOrbMaterial / createTslAgentCapsuleMaterial).
 *
 * The WebGPU path drives per-node emissive from a per-instance metadata texture
 * via TSL `emissiveNode`. WebGL has no node graph, so this injects equivalent
 * GLSL into the standard MeshStandardMaterial shader through `onBeforeCompile`:
 * each instance reads its metadata (RGBA float = quality, authority, connections,
 * recency) from an `aGlowMeta` InstancedBufferAttribute and reproduces the TSL
 * emissive/opacity maths exactly. Without this, every WebGL instance shared one
 * global `emissiveIntensity` (uniform breathing) while the WebGPU nodes glowed
 * individually — the visible renderer divergence (task #50).
 *
 * The injection stays GLSL ES 1.00 (Three's default for WebGL2 built-in
 * materials): `attribute`/`varying`, no `gl_InstanceID`, no vertex texture fetch.
 * Forcing GLSL3 to use `gl_InstanceID`/`texture()` breaks the built-in
 * `<opaque_fragment>` chunk (`gl_FragColor` undeclared) because Three does not
 * GLSL3-convert its own ShaderChunks for a built-in material. The metadata is fed
 * via `aGlowMeta` (vec4, shares the SAME Float32Array as the WebGPU DataTexture,
 * instance i at offset i*4) and `aInstanceIndex` (float, for the stable phase),
 * both set on the geometry in GemNodes (WebGL branch only).
 */
export interface GlslMetadataGlowOptions {
  /** Base emissive hue (matches the TSL `baseEmissive` per material). */
  baseEmissive: [number, number, number];
  /** Multiplier on the blue-channel subtraction from the connection warm-shift. */
  warmBlueMul: number;
  /** Fresnel-rim emissive hue ADDED on top (orb/agent). Omit for the gem (pure metadata). */
  fresnelEmissive?: [number, number, number];
  /** Authority → pulse-speed range (matches the TSL `mix(min,max,authority)`). */
  pulseSpeedMin: number;
  pulseSpeedMax: number;
  /** Apply the Fresnel + authority opacity node (transparent gem/orb). False for the opaque agent. */
  applyOpacity: boolean;
  /** Authority → base-alpha range (only used when applyOpacity). */
  baseAlphaMin?: number;
  baseAlphaMax?: number;
}

/** Material augmented with our shared glow-uniform handle for per-frame ticking. */
interface GlslGlowMaterial extends THREE.MeshStandardMaterial {
  userData: {
    glslGlowTime?: { value: number };
    glslGlowStrength?: { value: number };
    glslGlowApplied?: boolean;
  } & Record<string, unknown>;
}

/**
 * Inject per-instance metadata-driven emissive (and optional opacity) into an
 * existing MeshStandardMaterial. No-op on WebGPU (the TSL path owns that backend)
 * and idempotent (guarded by `userData.glslGlowApplied`).
 *
 * Reads per-instance metadata from the `aGlowMeta` InstancedBufferAttribute and
 * the phase seed from `aInstanceIndex` — both must be set on the mesh geometry
 * (GemNodes WebGL branch) before this material renders.
 *
 * @param material        the InstancedMesh material to augment
 * @param timeUniform     live `{ value }` ticked each frame (bind to GemNodes uniforms.time)
 * @param glowStrength    live `{ value }` for the per-type glow scale
 * @param opts            per-material palette / behaviour
 * @returns true when the augment was applied.
 */
export function applyGlslMetadataGlow(
  material: THREE.MeshStandardMaterial,
  timeUniform: { value: number },
  glowStrength: { value: number },
  opts: GlslMetadataGlowOptions,
): boolean {
  if (isWebGPURenderer) return false;

  const mat = material as GlslGlowMaterial;
  if (mat.userData.glslGlowApplied) return true;

  const [er, eg, eb] = opts.baseEmissive;
  const fres = opts.fresnelEmissive;
  const aMin = opts.baseAlphaMin ?? 0.55;
  const aMax = opts.baseAlphaMax ?? 0.85;

  // Stash the live uniform handles so the per-frame loop can tick them after the
  // shader is compiled (onBeforeCompile fires lazily on first render).
  mat.userData.glslGlowTime = timeUniform;
  mat.userData.glslGlowStrength = glowStrength;

  const prevOnBeforeCompile = mat.onBeforeCompile;
  mat.onBeforeCompile = (shader, renderer) => {
    if (prevOnBeforeCompile) prevOnBeforeCompile.call(mat, shader, renderer);

    shader.uniforms.uGlowTime = timeUniform;
    shader.uniforms.uGlowStrength = glowStrength;

    // --- Vertex: read per-instance metadata + derive a stable phase (GLSL1). ---
    shader.vertexShader = shader.vertexShader
      .replace(
        '#include <common>',
        `#include <common>
attribute vec4 aGlowMeta;
attribute float aInstanceIndex;
varying vec4 vGlowMeta;
varying float vGlowPhase;`,
      )
      .replace(
        '#include <begin_vertex>',
        `#include <begin_vertex>
vGlowMeta = aGlowMeta;
vGlowPhase = fract(sin(aInstanceIndex * 43758.5453)) * 6.2831;`,
      );

    // --- Fragment: reproduce the TSL emissive (+ optional opacity) maths. ---
    const fresnelDecl = `
  vec3 _vd = normalize(vViewPosition);
  float _ndv = clamp(dot(normalize(normal), _vd), 0.0, 1.0);
  float _fres = pow(1.0 - _ndv, 3.0);`;

    const fresnelEmissiveTerm = fres
      ? `vec3(${fres[0].toFixed(4)}, ${fres[1].toFixed(4)}, ${fres[2].toFixed(4)}) * _fres + `
      : '';

    shader.fragmentShader = shader.fragmentShader
      .replace(
        '#include <common>',
        `#include <common>
uniform float uGlowTime;
uniform float uGlowStrength;
varying vec4 vGlowMeta;
varying float vGlowPhase;`,
      )
      .replace(
        '#include <emissivemap_fragment>',
        `#include <emissivemap_fragment>
{
  float _q = vGlowMeta.x;
  float _au = vGlowMeta.y;
  float _cn = vGlowMeta.z;
  float _rc = vGlowMeta.w;${fresnelDecl}
  float _pSpeed = mix(${opts.pulseSpeedMin.toFixed(3)}, ${opts.pulseSpeedMax.toFixed(3)}, _au);
  float _pulse = sin(uGlowTime * _pSpeed + vGlowPhase) * 0.5 + 0.5;
  float _qB = mix(0.3, 0.8, _q);
  float _rB = mix(0.5, 1.0, _rc);
  float _warm = _cn * 0.25;
  vec3 _baseE = vec3(${er.toFixed(4)} + _warm, ${eg.toFixed(4)}, ${eb.toFixed(4)} - _warm * ${opts.warmBlueMul.toFixed(3)});
  vec3 _metaE = _baseE * _qB * mix(0.4, 1.0, _pulse) * _rB;
  totalEmissiveRadiance += (${fresnelEmissiveTerm}_metaE) * uGlowStrength;
}`,
      );

    if (opts.applyOpacity) {
      // Match the TSL opacityNode: Fresnel rim + authority-based solidity.
      shader.fragmentShader = shader.fragmentShader.replace(
        '#include <opaque_fragment>',
        `{
  float _auO = vGlowMeta.y;
  vec3 _vdO = normalize(vViewPosition);
  float _ndvO = clamp(dot(normalize(normal), _vdO), 0.0, 1.0);
  float _fresO = pow(1.0 - _ndvO, 3.0);
  float _baseA = mix(${aMin.toFixed(3)}, ${aMax.toFixed(3)}, _auO);
  diffuseColor.a *= mix(_baseA, 0.95, _fresO);
}
#include <opaque_fragment>`,
      );
    }
  };

  mat.userData.glslGlowApplied = true;
  mat.needsUpdate = true;
  logger.info('[GlslMetadataGlow] per-instance emissive injected (WebGL)', {
    applyOpacity: opts.applyOpacity,
  });
  return true;
}

/** Per-material palettes mirroring the three TSL augment functions. */
export const GLSL_GLOW_PRESETS: Record<'gem' | 'orb' | 'agent', GlslMetadataGlowOptions> = {
  gem: {
    baseEmissive: [0.25, 0.30, 0.50],
    warmBlueMul: 0.5,
    pulseSpeedMin: 0.8,
    pulseSpeedMax: 3.0,
    applyOpacity: true,
    baseAlphaMin: 0.55,
    baseAlphaMax: 0.85,
  },
  orb: {
    baseEmissive: [0.18, 0.18, 0.40],
    warmBlueMul: 0.5,
    fresnelEmissive: [0.12, 0.12, 0.25],
    pulseSpeedMin: 0.8,
    pulseSpeedMax: 3.0,
    applyOpacity: true,
    baseAlphaMin: 0.55,
    baseAlphaMax: 0.85,
  },
  agent: {
    baseEmissive: [0.08, 0.40, 0.20],
    warmBlueMul: 0.3,
    fresnelEmissive: [0.08, 0.40, 0.20],
    pulseSpeedMin: 1.0,
    pulseSpeedMax: 3.5,
    applyOpacity: false,
  },
};
