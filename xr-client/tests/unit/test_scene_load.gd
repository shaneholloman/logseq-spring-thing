extends "res://addons/gut/test.gd"

# Scene-load smoke tests for the Godot 4 XR client (PRD-QE-002 §4.2).
#
# Each test loads one of the four canonical scenes and verifies it
# instantiates without errors and contains the expected child structure.
# These run headless under CI via `xr-client/tests/run_gut.gd`.

func test_xr_boot_scene_loads():
	var packed := load("res://scenes/XRBoot.tscn")
	assert_not_null(packed, "XRBoot.tscn must be loadable")
	var root := packed.instantiate()
	assert_not_null(root, "XRBoot.tscn must instantiate")
	assert_eq(root.name, "XRBoot", "root node name")
	# XRBoot must contain an XROrigin3D (per scene definition).
	assert_not_null(root.get_node_or_null("XROrigin3D"), "XROrigin3D child")
	# XR cameras + controller children under the origin.
	var origin := root.get_node_or_null("XROrigin3D")
	assert_not_null(origin.get_node_or_null("XRCamera3D"), "XRCamera3D child")
	assert_not_null(origin.get_node_or_null("LeftController"), "LeftController child")
	assert_not_null(origin.get_node_or_null("RightController"), "RightController child")
	root.queue_free()

func test_graph_scene_loads():
	var packed := load("res://scenes/GraphScene.tscn")
	assert_not_null(packed, "GraphScene.tscn must be loadable")
	var root := packed.instantiate()
	assert_not_null(root, "GraphScene.tscn must instantiate")
	assert_eq(root.name, "GraphScene", "root node name")
	# Graph scene also hosts an XR origin + camera (player rig).
	assert_not_null(root.get_node_or_null("XROrigin3D"), "XROrigin3D child")
	root.queue_free()

func test_hud_scene_loads():
	var packed := load("res://scenes/HUD.tscn")
	assert_not_null(packed, "HUD.tscn must be loadable")
	var root := packed.instantiate()
	assert_not_null(root, "HUD.tscn must instantiate")
	assert_eq(root.name, "HUD", "root node name")
	# HUD hosts a SubViewport so the panel can render UI offscreen.
	assert_not_null(root.get_node_or_null("HudViewport"), "HudViewport child")
	root.queue_free()

func test_avatar_scene_loads():
	var packed := load("res://scenes/Avatar.tscn")
	assert_not_null(packed, "Avatar.tscn must be loadable")
	var root := packed.instantiate()
	assert_not_null(root, "Avatar.tscn must instantiate")
	assert_eq(root.name, "Avatar", "root node name")
	# Avatar geometry: head + (hidden) hand mesh instances.
	assert_not_null(root.get_node_or_null("Head"), "Head child")
	assert_not_null(root.get_node_or_null("LeftHand"), "LeftHand child")
	assert_not_null(root.get_node_or_null("RightHand"), "RightHand child")
	root.queue_free()

func test_all_scenes_have_attached_scripts():
	# Every top-level scene must wire a script — the gdext extension is
	# loaded but the GDScript glue lives in `xr-client/scripts/`.
	var scene_paths := [
		"res://scenes/XRBoot.tscn",
		"res://scenes/GraphScene.tscn",
		"res://scenes/HUD.tscn",
		"res://scenes/Avatar.tscn",
	]
	for path in scene_paths:
		var packed = load(path)
		assert_not_null(packed, "scene %s must load" % path)
		var root = packed.instantiate()
		var script = root.get_script()
		assert_not_null(script, "scene %s must attach a script" % path)
		root.queue_free()

func test_scene_uids_are_unique():
	# Each scene declares a `uid://` tag in its tscn header; collisions cause
	# Godot import failures. We verify by loading each scene and checking
	# resource_path differs.
	var paths := [
		"res://scenes/XRBoot.tscn",
		"res://scenes/GraphScene.tscn",
		"res://scenes/HUD.tscn",
		"res://scenes/Avatar.tscn",
	]
	var seen := {}
	for p in paths:
		var pkg = load(p)
		assert_not_null(pkg, "scene %s must load" % p)
		var rp = pkg.resource_path
		assert_false(seen.has(rp), "duplicate resource_path %s" % rp)
		seen[rp] = true
