extends Node3D

@onready var room_label: Label = $HudViewport/HudControl/VBox/RoomLabel
@onready var room_entry: LineEdit = $HudViewport/HudControl/VBox/RoomEntry
@onready var join_button: Button = $HudViewport/HudControl/VBox/JoinButton
@onready var mute_toggle: CheckButton = $HudViewport/HudControl/VBox/MuteToggle
@onready var debug_stats: Label = $HudViewport/HudControl/VBox/DebugStats

signal join_requested(room_urn: String)
signal mute_toggled(muted: bool)

var _avatar_count: int = 0
var _mtp_ms: float = 0.0
var _connected: bool = false


func _ready() -> void:
	join_button.pressed.connect(_on_join_pressed)
	mute_toggle.toggled.connect(_on_mute_toggled)
	set_process(true)


func _process(_delta: float) -> void:
	var conn_str: String = "OK" if _connected else "OFF"
	debug_stats.text = "FPS: %d  MTP: %.1fms  Avatars: %d  Net: %s" % [
		Engine.get_frames_per_second(),
		_mtp_ms,
		_avatar_count,
		conn_str,
	]


func set_avatar_count(count: int) -> void:
	_avatar_count = count


func set_mtp_ms(ms: float) -> void:
	_mtp_ms = ms


func _on_connection_status(connected: bool) -> void:
	_connected = connected


func _on_join_pressed() -> void:
	var urn := room_entry.text.strip_edges()
	if urn.is_empty():
		push_warning("Empty room URN")
		return
	emit_signal("join_requested", urn)
	room_label.text = "Room: %s" % urn


func _on_mute_toggled(state: bool) -> void:
	emit_signal("mute_toggled", state)
