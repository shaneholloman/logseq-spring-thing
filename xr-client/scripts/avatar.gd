extends Node3D

## Default hand offsets relative to head when full hand tracking data is unavailable.
const DEFAULT_HAND_OFFSET_Y: float = -0.4
const DEFAULT_HAND_OFFSET_X: float = 0.25
const INTERPOLATION_SPEED: float = 12.0

@onready var head: MeshInstance3D = $Head
@onready var left_hand: MeshInstance3D = $LeftHand
@onready var right_hand: MeshInstance3D = $RightHand
@onready var nameplate: Label3D = $Head/Nameplate
@onready var voice_indicator: MeshInstance3D = $Head/VoiceIndicator

## Current LOD level (0=High, 1=Med, 2=Low, 3=Culled).
var _lod_level: int = 0

## Target transforms for interpolation.
var _target_head_pos: Vector3 = Vector3.ZERO
var _target_head_basis: Basis = Basis.IDENTITY
var _target_left_pos: Vector3 = Vector3.ZERO
var _target_left_basis: Basis = Basis.IDENTITY
var _target_right_pos: Vector3 = Vector3.ZERO
var _target_right_basis: Basis = Basis.IDENTITY
var _has_left: bool = false
var _has_right: bool = false
var _has_received_pose: bool = false


func set_display_name(display_name: String) -> void:
	if nameplate != null:
		nameplate.text = display_name


func apply_pose(
	head_pos: Vector3,
	head_rot: Quaternion,
	has_left: bool,
	has_right: bool,
	left_pos: Vector3 = Vector3.ZERO,
	left_rot: Quaternion = Quaternion.IDENTITY,
	right_pos: Vector3 = Vector3.ZERO,
	right_rot: Quaternion = Quaternion.IDENTITY
) -> void:
	_has_received_pose = true
	_target_head_pos = head_pos
	_target_head_basis = Basis(head_rot)
	_has_left = has_left
	_has_right = has_right

	if has_left:
		if left_pos == Vector3.ZERO and left_rot == Quaternion.IDENTITY:
			_target_left_pos = head_pos + Vector3(-DEFAULT_HAND_OFFSET_X, DEFAULT_HAND_OFFSET_Y, 0.0)
			_target_left_basis = Basis(head_rot)
		else:
			_target_left_pos = left_pos
			_target_left_basis = Basis(left_rot)

	if has_right:
		if right_pos == Vector3.ZERO and right_rot == Quaternion.IDENTITY:
			_target_right_pos = head_pos + Vector3(DEFAULT_HAND_OFFSET_X, DEFAULT_HAND_OFFSET_Y, 0.0)
			_target_right_basis = Basis(head_rot)
		else:
			_target_right_pos = right_pos
			_target_right_basis = Basis(right_rot)


func _process(delta: float) -> void:
	if not _has_received_pose:
		return
	var weight: float = clampf(INTERPOLATION_SPEED * delta, 0.0, 1.0)

	if head != null:
		head.transform.origin = head.transform.origin.lerp(_target_head_pos, weight)
		head.transform.basis = head.transform.basis.slerp(_target_head_basis, weight)

	if left_hand != null:
		left_hand.visible = _has_left and _lod_level < 2
		if _has_left:
			left_hand.transform.origin = left_hand.transform.origin.lerp(_target_left_pos, weight)
			left_hand.transform.basis = left_hand.transform.basis.slerp(_target_left_basis, weight)

	if right_hand != null:
		right_hand.visible = _has_right and _lod_level < 2
		if _has_right:
			right_hand.transform.origin = right_hand.transform.origin.lerp(_target_right_pos, weight)
			right_hand.transform.basis = right_hand.transform.basis.slerp(_target_right_basis, weight)


func set_lod_level(level: int) -> void:
	_lod_level = level
	match level:
		0:  # High -- everything visible
			visible = true
			if nameplate != null:
				nameplate.visible = true
			if left_hand != null:
				left_hand.visible = _has_left
			if right_hand != null:
				right_hand.visible = _has_right
		1:  # Medium -- hide nameplate
			visible = true
			if nameplate != null:
				nameplate.visible = false
			if left_hand != null:
				left_hand.visible = _has_left
			if right_hand != null:
				right_hand.visible = _has_right
		2:  # Low -- hide nameplate and hands
			visible = true
			if nameplate != null:
				nameplate.visible = false
			if left_hand != null:
				left_hand.visible = false
			if right_hand != null:
				right_hand.visible = false
		3:  # Culled -- hide entirely
			visible = false


func set_speaking(active: bool) -> void:
	if voice_indicator != null:
		voice_indicator.visible = active
