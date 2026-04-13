/**
 * updateHandTrackingFromSession
 *
 * Reads XR input sources from an active XRSession and updates
 * hand tracking state for both primary (right) and secondary (left) hands.
 * Supports full hand tracking (Quest hand tracking) and controller input.
 */

import type { HandState } from './useVRHandTracking';

export function updateHandTrackingFromSession(
  session: XRSession,
  updateHandState: (hand: 'primary' | 'secondary', state: Partial<HandState>) => void
): void {
  const inputSources = session.inputSources;
  if (!inputSources) return;

  for (const source of Array.from(inputSources) as XRInputSource[]) {
    const hand = source.handedness === 'right' ? 'primary' : 'secondary';

    if (source.hand) {
      // Full hand tracking (Quest hand tracking)
      updateHandState(hand, {
        isTracking: true,
        isPointing: true,
      });
    } else if (source.gamepad) {
      // Controller tracking
      const isPointing =
        source.gamepad.buttons[0]?.pressed || source.gamepad.buttons[1]?.pressed;

      updateHandState(hand, {
        isTracking: true,
        isPointing,
        pinchStrength: Math.max(
          source.gamepad.buttons[0]?.value || 0,
          source.gamepad.buttons[1]?.value || 0
        ),
      });
    }
  }
}
