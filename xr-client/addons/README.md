# Godot Addons

Install via Godot AssetLib at first project open. Pinned versions:

| Addon | Version | Source |
|---|---|---|
| Godot OpenXR Vendors | 3.0.x | https://github.com/GodotVR/godot_openxr_vendors |
| Godot OpenXR Loaders (Khronos + Meta + Pico) | bundled with Vendors plugin | (same) |

The addons themselves are not committed; CI runs `godot --headless --import` first
which populates `addons/` from AssetLib. After install, the directory contains:

```
addons/
  godot_openxr_vendors/
    plugin.cfg
    config/
    extension/
```

LiveKit Android AAR (PRD-008 §5.5) lands here in a follow-up sprint.
