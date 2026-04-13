import { createXRStore } from '@react-three/xr';

// Single XR store instance shared across all VR components.
// Disable XR emulation in production to avoid bundling @iwer/sem room scene
// data (~4.6MB of MetaQuest scene captures used only for localhost dev).
export const xrStore = createXRStore({
  hand: true,
  controller: true,
  emulate: import.meta.env.DEV ? 'metaQuest3' : false,
});
