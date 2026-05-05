extends Node3D

const AVATAR_TEMPLATE_PATH := "res://scenes/Avatar.tscn"
const RECONNECT_DELAY_SEC: float = 2.0
const MAX_RECONNECT_ATTEMPTS: int = 3

var _binary_client: RefCounted = null
var _presence_client: RefCounted = null
var _interaction: RefCounted = null
var _lod_policy: RefCounted = null
var _voice_router: RefCounted = null

var _avatars: Dictionary = {}
var _node_positions: Dictionary = {}

var _graph_ws_url: String = ""
var _presence_ws_url: String = ""
var _room_urn: String = ""
var _display_name: String = ""
var _reconnect_attempts: int = 0
var _reconnect_timer: float = -1.0

@onready var nodes_multi: MultiMeshInstance3D = $NodesMulti
@onready var avatar_spawner: Node3D = $AvatarSpawner

signal node_targeted_in_scene(node_id: int)
signal avatar_count_changed(count: int)
signal connection_status_changed(connected: bool)


func _ready() -> void:
	_binary_client = ClassDB.instantiate("BinaryProtocolClient") if ClassDB.class_exists("BinaryProtocolClient") else null
	_presence_client = ClassDB.instantiate("PresenceClientNode") if ClassDB.class_exists("PresenceClientNode") else null
	_interaction = ClassDB.instantiate("XrInteraction") if ClassDB.class_exists("XrInteraction") else null
	_lod_policy = ClassDB.instantiate("LodPolicy") if ClassDB.class_exists("LodPolicy") else null
	_voice_router = ClassDB.instantiate("SpatialVoiceRouter") if ClassDB.class_exists("SpatialVoiceRouter") else null

	if _binary_client != null:
		_binary_client.connect("position_updated", Callable(self, "_on_position_updated"))
	if _presence_client != null:
		_presence_client.connect("avatar_joined", Callable(self, "_on_avatar_joined"))
		_presence_client.connect("avatar_left", Callable(self, "_on_avatar_left"))
		_presence_client.connect("avatar_pose_updated", Callable(self, "_on_avatar_pose_updated"))
		if _presence_client.has_signal("presence_kicked"):
			_presence_client.connect("presence_kicked", Callable(self, "_on_presence_kicked"))
	if _voice_router != null and _voice_router.has_signal("voice_activity"):
		_voice_router.connect("voice_activity", Callable(self, "_on_voice_activity"))
	if _interaction != null:
		_interaction.connect("node_targeted", Callable(self, "_on_node_targeted"))
		_interaction.connect("node_grabbed", Callable(self, "_on_node_grabbed"))


func connect_to_server(graph_ws_url: String, presence_ws_url: String, room_urn: String, display_name: String) -> void:
	_graph_ws_url = graph_ws_url
	_presence_ws_url = presence_ws_url
	_room_urn = room_urn
	_display_name = display_name
	_reconnect_attempts = 0
	_attempt_connect()


func _attempt_connect() -> void:
	if _binary_client != null and _binary_client.has_method("connect_to_url"):
		_binary_client.connect_to_url(_graph_ws_url)
	if _presence_client != null and _presence_client.has_method("join"):
		_presence_client.join(_presence_ws_url, _room_urn, _display_name)
	emit_signal("connection_status_changed", true)


func _physics_process(delta: float) -> void:
	_update_lod()
	_update_multimesh()
	_update_voice_listener()
	_tick_reconnect(delta)


func _update_lod() -> void:
	if _lod_policy == null or not _lod_policy.has_method("should_recompute"):
		return
	if not _lod_policy.should_recompute():
		return
	var camera: XRCamera3D = _find_xr_camera()
	if camera == null:
		return
	var cam_pos: Vector3 = camera.global_position
	for avatar_id: String in _avatars:
		var av: Node3D = _avatars[avatar_id]
		var dist: float = cam_pos.distance_to(av.global_position)
		var level: int = _lod_policy.classify_distance(dist)
		if av.has_method("set_lod_level"):
			av.set_lod_level(level)


func _update_multimesh() -> void:
	if nodes_multi == null or nodes_multi.multimesh == null:
		return
	var mm: MultiMesh = nodes_multi.multimesh
	var ids: Array = _node_positions.keys()
	var count: int = ids.size()
	if mm.instance_count != count:
		mm.instance_count = count
	for i: int in range(count):
		var pos: Vector3 = _node_positions[ids[i]]
		var xf := Transform3D(Basis.IDENTITY, pos)
		mm.set_instance_transform(i, xf)


func _update_voice_listener() -> void:
	if _voice_router == null or not _voice_router.has_method("update_listener"):
		return
	var camera: XRCamera3D = _find_xr_camera()
	if camera == null:
		return
	var cam_pos: Vector3 = camera.global_position
	var cam_fwd: Vector3 = -camera.global_transform.basis.z
	var cam_up: Vector3 = camera.global_transform.basis.y
	_voice_router.update_listener(cam_pos, cam_fwd, cam_up)


func _tick_reconnect(delta: float) -> void:
	if _reconnect_timer < 0.0:
		return
	_reconnect_timer -= delta
	if _reconnect_timer <= 0.0:
		_reconnect_timer = -1.0
		_attempt_connect()


func _schedule_reconnect() -> void:
	if _reconnect_attempts >= MAX_RECONNECT_ATTEMPTS:
		push_warning("GraphScene: max reconnect attempts reached (%d)" % MAX_RECONNECT_ATTEMPTS)
		emit_signal("connection_status_changed", false)
		return
	_reconnect_attempts += 1
	_reconnect_timer = RECONNECT_DELAY_SEC
	push_warning("GraphScene: reconnect attempt %d/%d in %.1fs" % [_reconnect_attempts, MAX_RECONNECT_ATTEMPTS, RECONNECT_DELAY_SEC])


func _find_xr_camera() -> XRCamera3D:
	var origin: XROrigin3D = XRServer.get_reference_frame() as XROrigin3D if XRServer.has_method("get_reference_frame") else null
	if origin != null:
		for child: Node in origin.get_children():
			if child is XRCamera3D:
				return child as XRCamera3D
	var viewport_cam: Camera3D = get_viewport().get_camera_3d()
	if viewport_cam is XRCamera3D:
		return viewport_cam as XRCamera3D
	return null


func _on_position_updated(node_id: int, position: Vector3, _velocity: Vector3) -> void:
	_node_positions[node_id] = position


func _on_avatar_joined(did: String, display_name: String, avatar_id: String) -> void:
	var template := load(AVATAR_TEMPLATE_PATH)
	if template == null:
		push_warning("Avatar template missing")
		return
	var avatar := template.instantiate()
	avatar_spawner.add_child(avatar)
	avatar.set_meta("avatar_id", avatar_id)
	avatar.set_meta("did", did)
	if avatar.has_method("set_display_name"):
		avatar.set_display_name(display_name)
	_avatars[avatar_id] = avatar

	if _voice_router != null and _voice_router.has_method("attach_track"):
		_voice_router.attach_track(did, avatar.global_position)

	emit_signal("avatar_count_changed", _avatars.size())


func _on_avatar_left(avatar_id: String) -> void:
	if not _avatars.has(avatar_id):
		return
	var av: Node = _avatars[avatar_id]
	var did: String = av.get_meta("did", "")
	if _voice_router != null and _voice_router.has_method("detach_track") and did != "":
		_voice_router.detach_track(did)
	_avatars.erase(avatar_id)
	av.queue_free()
	emit_signal("avatar_count_changed", _avatars.size())


func _on_avatar_pose_updated(
	avatar_id: String,
	head_pos: Vector3,
	head_rot: Quaternion,
	has_left: bool,
	has_right: bool
) -> void:
	if not _avatars.has(avatar_id):
		return
	var av: Node3D = _avatars[avatar_id]
	if av.has_method("apply_pose"):
		av.apply_pose(head_pos, head_rot, has_left, has_right)


func _on_voice_activity(avatar_id: String, active: bool) -> void:
	if not _avatars.has(avatar_id):
		return
	var av: Node3D = _avatars[avatar_id]
	if av.has_method("set_speaking"):
		av.set_speaking(active)


func _on_node_targeted(node_id: int, _distance: float) -> void:
	emit_signal("node_targeted_in_scene", node_id)


func _on_node_grabbed(node_id: int, position: Vector3) -> void:
	_node_positions[node_id] = position


func _on_presence_kicked(reason: String) -> void:
	push_warning("GraphScene: kicked from presence -- %s" % reason)
	_schedule_reconnect()
