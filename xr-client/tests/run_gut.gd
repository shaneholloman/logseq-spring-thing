extends SceneTree

# Headless gut runner. PRD-QE-002 §4.2 specifies gut as the GDScript test
# framework; the CI installs it under res://addons/gut/. This script boots
# gut, runs every test under res://tests/unit/, writes a JUnit report under
# res://tests/report/, and exits with a non-zero status on any failure.

const REPORT_DIR := "res://tests/report"
const TESTS_DIR := "res://tests/unit"

func _init() -> void:
	var gut_class := load("res://addons/gut/gut.gd")
	if gut_class == null:
		printerr("gut framework not installed at res://addons/gut/")
		quit(2)
		return

	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(REPORT_DIR))

	var gut := gut_class.new()
	get_root().add_child(gut)
	gut.set_log_level(2)
	gut.add_directory(TESTS_DIR)
	gut.set_junit_xml_file(REPORT_DIR.path_join("junit.xml"))
	gut.connect("tests_finished", Callable(self, "_on_tests_finished").bind(gut))
	gut.test_scripts()

func _on_tests_finished(gut) -> void:
	var failed := gut.get_fail_count() + gut.get_pending_count()
	if failed > 0:
		printerr("gut: %d failed / %d pending" % [gut.get_fail_count(), gut.get_pending_count()])
		quit(1)
	else:
		print("gut: all %d tests passed" % gut.get_pass_count())
		quit(0)
