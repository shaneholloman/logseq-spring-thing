extends Node3D

@onready var head: MeshInstance3D = $Head
@onready var left_hand: MeshInstance3D = $LeftHand
@onready var right_hand: MeshInstance3D = $RightHand
@onready var nameplate: Label3D = $Head/Nameplate
@onready var voice_indicator: MeshInstance3D = $Head/VoiceIndicator

func set_display_name(display_name: String) -> void:
	if nameplate != null:
		nameplate.text = display_name

func apply_pose(head_pos: Vector3, head_rot: Quaternion, has_left: bool, has_right: bool) -> void:
	if head != null:
		head.transform.origin = head_pos
		head.transform.basis = Basis(head_rot)
	if left_hand != null:
		left_hand.visible = has_left
	if right_hand != null:
		right_hand.visible = has_right

func set_speaking(active: bool) -> void:
	if voice_indicator != null:
		voice_indicator.visible = active
