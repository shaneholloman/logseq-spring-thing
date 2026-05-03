extends Node3D

const AVATAR_TEMPLATE_PATH := "res://scenes/Avatar.tscn"

var _binary_client: RefCounted = null
var _presence_client: RefCounted = null
var _interaction: RefCounted = null
var _lod_policy: RefCounted = null
var _voice_router: RefCounted = null

var _avatars: Dictionary = {}
var _node_positions: Dictionary = {}

@onready var nodes_multi: MultiMeshInstance3D = $NodesMulti
@onready var avatar_spawner: Node3D = $AvatarSpawner

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
	if _interaction != null:
		_interaction.connect("node_targeted", Callable(self, "_on_node_targeted"))
		_interaction.connect("node_grabbed", Callable(self, "_on_node_grabbed"))

func _on_position_updated(node_id: int, position: Vector3, _velocity: Vector3) -> void:
	_node_positions[node_id] = position

func _on_avatar_joined(_did: String, display_name: String, avatar_id: String) -> void:
	var template := load(AVATAR_TEMPLATE_PATH)
	if template == null:
		push_warning("Avatar template missing")
		return
	var avatar := template.instantiate()
	avatar_spawner.add_child(avatar)
	avatar.set_meta("avatar_id", avatar_id)
	if avatar.has_method("set_display_name"):
		avatar.set_display_name(display_name)
	_avatars[avatar_id] = avatar

func _on_avatar_left(avatar_id: String) -> void:
	if _avatars.has(avatar_id):
		var av: Node = _avatars[avatar_id]
		_avatars.erase(avatar_id)
		av.queue_free()

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

func _on_node_targeted(node_id: int, _distance: float) -> void:
	emit_signal("node_targeted_in_scene", node_id)

func _on_node_grabbed(node_id: int, position: Vector3) -> void:
	_node_positions[node_id] = position

signal node_targeted_in_scene(node_id: int)
