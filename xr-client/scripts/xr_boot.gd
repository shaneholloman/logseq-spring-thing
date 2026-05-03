extends Node3D

@onready var error_overlay: Node3D = $ErrorOverlay
@onready var error_label: Label3D = $ErrorOverlay/ErrorLabel

func _ready() -> void:
	var xr_interface: XRInterface = XRServer.find_interface("OpenXR")
	if xr_interface == null:
		_show_error("OpenXR runtime not present.")
		return
	if not xr_interface.is_initialized():
		if not xr_interface.initialize():
			_show_error("OpenXR initialise() returned false.")
			return
	get_viewport().use_xr = true
	if not _probe_capabilities(xr_interface):
		return
	_transition_to_graph_scene()

func _probe_capabilities(xr_interface: XRInterface) -> bool:
	var ok := true
	var msg := PackedStringArray()
	if not xr_interface.has_method("get_capabilities"):
		msg.append("XR interface lacks get_capabilities().")
	if not xr_interface.is_passthrough_supported():
		msg.append("Passthrough unsupported.")
		ok = false
	if not ok:
		_show_error("\n".join(msg))
	return ok

func _transition_to_graph_scene() -> void:
	var graph_scene := load("res://scenes/GraphScene.tscn")
	if graph_scene == null:
		_show_error("GraphScene.tscn missing.")
		return
	get_tree().change_scene_to_packed(graph_scene)

func _show_error(text: String) -> void:
	error_overlay.visible = true
	error_label.text = text
	push_error("XRBoot: %s" % text)
