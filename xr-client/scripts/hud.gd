extends Node3D

@onready var room_label: Label = $HudViewport/HudControl/VBox/RoomLabel
@onready var room_entry: LineEdit = $HudViewport/HudControl/VBox/RoomEntry
@onready var join_button: Button = $HudViewport/HudControl/VBox/JoinButton
@onready var mute_toggle: CheckButton = $HudViewport/HudControl/VBox/MuteToggle
@onready var debug_stats: Label = $HudViewport/HudControl/VBox/DebugStats

signal join_requested(room_urn: String)
signal mute_toggled(muted: bool)

func _ready() -> void:
	join_button.pressed.connect(_on_join_pressed)
	mute_toggle.toggled.connect(_on_mute_toggled)
	set_process(true)

func _process(_delta: float) -> void:
	debug_stats.text = "FPS: %d  MTP: --ms  Avatars: --" % Engine.get_frames_per_second()

func _on_join_pressed() -> void:
	var urn := room_entry.text.strip_edges()
	if urn.is_empty():
		push_warning("Empty room URN")
		return
	emit_signal("join_requested", urn)
	room_label.text = "Room: %s" % urn

func _on_mute_toggled(state: bool) -> void:
	emit_signal("mute_toggled", state)
