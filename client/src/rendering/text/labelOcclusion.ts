/**
 * labelOcclusion.ts — screen-space occlusion for HTML-overlay labels.
 *
 * The WebGPU label path renders labels as DOM `<div>` overlays (drei `<Html>`),
 * which the browser compositor always paints on top of the canvas — they cannot
 * be depth-tested against the 3D scene. To restore the intent that a node's
 * label is hidden when the node is behind another node, we approximate occlusion
 * in screen space: a label is occluded when a nearer node's projected disc
 * covers this node's screen centre.
 *
 * This is a deliberate approximation (node-vs-node only; edges and arbitrary
 * geometry are ignored) because node-behind-node is the visually dominant case
 * and the one users notice. It is renderer-independent pure logic so it is unit
 * tested directly.
 */

export interface OcclusionCandidate {
  /** Node centre, projected to canvas pixels (origin top-left). */
  screenX: number;
  screenY: number;
  /** Node visual radius projected to pixels at its distance. */
  screenRadius: number;
  /** Distance from camera to node centre (world units). */
  distance: number;
}

/**
 * Returns a boolean per candidate: true when that candidate's node is occluded
 * by a nearer node whose screen disc covers it.
 *
 * @param depthBias minimum distance a blocker must be in front (world units)
 *   before it counts as an occluder — avoids flicker between co-planar nodes.
 */
export function computeOcclusionMask(
  candidates: ReadonlyArray<OcclusionCandidate>,
  depthBias = 0.5,
): boolean[] {
  const n = candidates.length;
  const occluded = new Array<boolean>(n).fill(false);

  for (let a = 0; a < n; a++) {
    const A = candidates[a];
    for (let b = 0; b < n; b++) {
      if (a === b) continue;
      const B = candidates[b];
      // B must be clearly in front of A to occlude it.
      if (B.distance >= A.distance - depthBias) continue;
      // A's centre must fall inside B's projected disc.
      const dx = A.screenX - B.screenX;
      const dy = A.screenY - B.screenY;
      if (dx * dx + dy * dy < B.screenRadius * B.screenRadius) {
        occluded[a] = true;
        break;
      }
    }
  }

  return occluded;
}
