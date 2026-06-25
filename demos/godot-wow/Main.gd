extends Node3D
## opcusdb Townfall — Godot 4 client for the Rust town server (demos/server/src/wow.rs).
##
## A tiny 3D MMO town: NPC quest givers, wolves to kill, quests, and chat, all over
## a WebSocket to the authoritative server. Other connected players appear in your
## world and move/fight in real time.
##
## Controls: WASD/arrows move · Space attack · E talk to NPC · Enter chat.

const URL := "ws://localhost:9007"

var ws := WebSocketPeer.new()
var connected := false
var my_id := 0
var player_name := "Hero"

var players := {}   # id -> {root, tx, tz, tyaw, name}
var wolves := {}    # id -> {root, tx, tz, tyaw, state}
var npcs := {}      # idx -> root
var quest := 0
var prog := 0
var kills := 0
var hp := 100.0
var last_keys := ""

var cam: Camera3D
var quest_label: Label
var status_label: Label
var chat_log: RichTextLabel
var chat_input: LineEdit
var chat_lines: Array = []

# headless screenshot support
var capture_mode := false
var capture_path := ""
var captured := false
var elapsed := 0.0
var did_interact := false
var _s0 := false
var _s1 := false
var _s2 := false
var _msg_time := 0.0
var _last_evt_time := -1.0

const PCOLORS := [Color("4f8cff"), Color("ff5d5d"), Color("57d977"), Color("ffd24a"), Color("c07dff"), Color("ff9f5a"), Color("4de1e6"), Color("f75fb4")]

const SKILL_MAX := [3.0, 7.0, 12.0]
const SKILL_NAMES := ["Cleave", "Fireball", "Heal"]
const SKILL_ICON := ["🪓", "🔥", "✚"]
const SKILL_COL := [Color("e07b39"), Color("d83a3a"), Color("3aa657")]
var skill_cd := [0.0, 0.0, 0.0]
var skill_ui: Array = []   # [{mat, cdl}]

func _ready() -> void:
	capture_path = OS.get_environment("WOW_SHOT")
	capture_mode = capture_path != ""
	if not capture_mode:
		player_name = "Hero" + str(randi() % 900 + 100)
	_build_world()
	_build_ui()
	ws.connect_to_url(URL)

# ---------- world ----------
func _mat(c: Color, rough := 1.0) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.roughness = rough
	m.metallic = 0.0
	return m

func _box(size: Vector3, c: Color, pos: Vector3, parent: Node3D) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	var bm := BoxMesh.new(); bm.size = size; mi.mesh = bm
	mi.material_override = _mat(c); mi.position = pos
	parent.add_child(mi)
	return mi

func _build_world() -> void:
	var env := WorldEnvironment.new()
	var e := Environment.new()
	e.background_mode = Environment.BG_COLOR
	e.background_color = Color(0.55, 0.66, 0.85)
	e.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	e.ambient_light_color = Color(0.6, 0.62, 0.7)
	e.ambient_light_energy = 0.9
	e.fog_enabled = true
	e.fog_light_color = Color(0.6, 0.7, 0.88)
	e.fog_density = 0.010
	env.environment = e
	add_child(env)

	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-52, -38, 0)
	sun.light_energy = 1.15
	sun.shadow_enabled = true
	add_child(sun)

	cam = Camera3D.new()
	cam.fov = 60
	add_child(cam)

	var ground := MeshInstance3D.new()
	var pm := PlaneMesh.new(); pm.size = Vector2(90, 90); ground.mesh = pm
	ground.material_override = _mat(Color(0.31, 0.46, 0.22))
	add_child(ground)

	# town square (dirt)
	_box(Vector3(20, 0.12, 18), Color(0.56, 0.47, 0.34), Vector3(0, 0.06, 3), self)
	# fountain
	_box(Vector3(3, 0.8, 3), Color(0.62, 0.64, 0.68), Vector3(0, 0.4, 3), self)
	_box(Vector3(1.4, 1.2, 1.4), Color(0.5, 0.72, 0.85), Vector3(0, 0.9, 3), self)

	for h in [Vector3(-11, 0, 5), Vector3(11, 0, 5), Vector3(-8, 0, 9), Vector3(9, 0, 9)]:
		_house(h)
	# forest edge to the north (where wolves roam)
	for i in range(16):
		_tree(Vector3(-20 + i * 2.7, 0, -24 + (i % 4) * 1.6))
	# a couple of fences framing the square
	for i in range(10):
		_box(Vector3(0.3, 1.0, 1.6), Color(0.45, 0.32, 0.2), Vector3(-10 + i * 2.2, 0.5, -6), self)

func _house(pos: Vector3) -> void:
	var root := Node3D.new(); root.position = pos; add_child(root)
	_box(Vector3(4, 3, 4), Color(0.80, 0.70, 0.52), Vector3(0, 1.5, 0), root)        # walls
	_box(Vector3(4.6, 1.2, 4.6), Color(0.62, 0.24, 0.18), Vector3(0, 3.4, 0), root)  # roof
	_box(Vector3(0.9, 1.6, 0.2), Color(0.4, 0.26, 0.16), Vector3(0, 0.8, 2.05), root) # door

func _tree(pos: Vector3) -> void:
	var root := Node3D.new(); root.position = pos; add_child(root)
	_box(Vector3(0.5, 2.0, 0.5), Color(0.36, 0.25, 0.15), Vector3(0, 1.0, 0), root)
	_box(Vector3(2.4, 2.4, 2.4), Color(0.18, 0.42, 0.22), Vector3(0, 3.0, 0), root)

func _label3d(text: String, color: Color, y: float, parent: Node3D, size := 48) -> Label3D:
	var l := Label3D.new()
	l.text = text
	l.modulate = color
	l.font_size = size
	l.outline_size = 8
	l.position = Vector3(0, y, 0)
	l.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	l.no_depth_test = true
	parent.add_child(l)
	return l

func _make_player(id: int, pname: String) -> Dictionary:
	var root := Node3D.new(); add_child(root)
	var col: Color = PCOLORS[(id - 1) % PCOLORS.size()]
	var mi := MeshInstance3D.new()
	var cm := CapsuleMesh.new(); cm.radius = 0.45; cm.height = 1.5; mi.mesh = cm
	mi.material_override = _mat(col); mi.position = Vector3(0, 0.95, 0)
	root.add_child(mi)
	_box(Vector3(0.5, 0.5, 0.2), Color(0.95, 0.85, 0.6), Vector3(0, 1.35, -0.4), root) # face
	# sword on a pivot so it can swing
	var arm := Node3D.new(); arm.position = Vector3(0.55, 1.1, -0.1); root.add_child(arm)
	_box(Vector3(0.16, 1.0, 0.16), Color(0.85, 0.85, 0.92), Vector3(0, 0.4, 0), arm)
	var nm := pname + (" (you)" if id == my_id else "")
	_label3d(nm, Color.WHITE if id != my_id else Color("9fe0ff"), 2.3, root)
	return {"root": root, "tx": 0.0, "tz": 0.0, "tyaw": 0.0, "name": pname, "arm": arm, "swing": 0.0}

func _make_wolf(id: int) -> Dictionary:
	var root := Node3D.new(); add_child(root)
	var grey := Color(0.32, 0.33, 0.36)
	_box(Vector3(1.4, 0.8, 0.7), grey, Vector3(0, 0.55, 0), root)            # body
	_box(Vector3(0.6, 0.6, 0.6), Color(0.26, 0.27, 0.3), Vector3(0, 0.8, -0.7), root) # head
	_box(Vector3(0.16, 0.3, 0.16), grey, Vector3(-0.18, 1.15, -0.8), root)   # ears
	_box(Vector3(0.16, 0.3, 0.16), grey, Vector3(0.18, 1.15, -0.8), root)
	_box(Vector3(0.1, 0.1, 0.1), Color(1, 0.2, 0.15), Vector3(-0.16, 0.85, -1.0), root) # eyes
	_box(Vector3(0.1, 0.1, 0.1), Color(1, 0.2, 0.15), Vector3(0.16, 0.85, -1.0), root)
	for legx in [-0.45, 0.45]:
		for legz in [-0.4, 0.4]:
			_box(Vector3(0.18, 0.5, 0.18), Color(0.24, 0.25, 0.28), Vector3(legx, 0.25, legz), root)
	# floating HP bar (hidden until hurt)
	var hpbg := _box(Vector3(1.1, 0.16, 0.06), Color(0, 0, 0), Vector3(0, 1.7, 0), root)
	var hpfg := _box(Vector3(1.05, 0.13, 0.08), Color(1, 0.25, 0.2), Vector3(0, 1.7, 0.01), root)
	hpbg.visible = false; hpfg.visible = false
	return {"root": root, "tx": 0.0, "tz": 0.0, "tyaw": 0.0, "state": 0, "hp": 1.0, "hpbg": hpbg, "hpfg": hpfg}

func _make_npc(idx: int, name: String, x: float, z: float, giver: bool) -> Node3D:
	var root := Node3D.new(); root.position = Vector3(x, 0, z); add_child(root)
	var col := Color(0.9, 0.78, 0.3) if giver else Color(0.6, 0.8, 0.95)
	var mi := MeshInstance3D.new()
	var cm := CapsuleMesh.new(); cm.radius = 0.45; cm.height = 1.6; mi.mesh = cm
	mi.material_override = _mat(col); mi.position = Vector3(0, 1.0, 0)
	root.add_child(mi)
	_box(Vector3(0.5, 0.5, 0.2), Color(0.95, 0.85, 0.6), Vector3(0, 1.45, -0.4), root)
	_label3d(name, Color("ffe9a8"), 2.5, root, 44)
	if giver:
		var bang := _label3d("!", Color("ffd000"), 3.4, root, 96)
		bang.outline_size = 12
	return root

# ---------- UI ----------
func _build_ui() -> void:
	# A full-rect Control root so the UI scales with the window (stretch=canvas_items).
	var ui := Control.new()
	ui.set_anchors_preset(Control.PRESET_FULL_RECT)
	ui.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(ui)
	var vp := Vector2(1280, 720) # base resolution; everything scales from here

	var title := Label.new()
	title.text = "opcusdb Townfall"
	title.position = Vector2(20, 14)
	title.add_theme_font_size_override("font_size", 32)
	ui.add_child(title)

	status_label = Label.new()
	status_label.position = Vector2(20, 56)
	status_label.add_theme_font_size_override("font_size", 22)
	status_label.add_theme_color_override("font_color", Color("9fe0ff"))
	ui.add_child(status_label)

	var qpanel := Panel.new()
	qpanel.position = Vector2(20, 92); qpanel.size = Vector2(470, 76)
	ui.add_child(qpanel)
	quest_label = Label.new()
	quest_label.position = Vector2(16, 12); quest_label.size = Vector2(438, 56)
	quest_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	quest_label.add_theme_font_size_override("font_size", 24)
	quest_label.add_theme_color_override("font_color", Color("ffd24a"))
	qpanel.add_child(quest_label)

	# Controls panel (top-right) — clearly shows how to attack
	var cpanel := Panel.new()
	cpanel.size = Vector2(330, 152); cpanel.position = Vector2(vp.x - 350, 14)
	ui.add_child(cpanel)
	var chead := Label.new()
	chead.text = "CONTROLS"; chead.position = Vector2(16, 10)
	chead.add_theme_font_size_override("font_size", 18)
	chead.add_theme_color_override("font_color", Color("8fa1c4"))
	cpanel.add_child(chead)
	var ctl := Label.new()
	ctl.position = Vector2(16, 38); ctl.size = Vector2(300, 110)
	ctl.add_theme_font_size_override("font_size", 22)
	ctl.text = "WASD / arrows — move\n[SPACE] — Attack ⚔\n[E] — Talk to NPC\n[ENTER] — Chat"
	cpanel.add_child(ctl)

	chat_log = RichTextLabel.new()
	chat_log.bbcode_enabled = true
	chat_log.scroll_following = true
	chat_log.size = Vector2(560, 200); chat_log.position = Vector2(20, vp.y - 250)
	chat_log.add_theme_font_size_override("normal_font_size", 20)
	chat_log.add_theme_font_size_override("bold_font_size", 20)
	ui.add_child(chat_log)
	chat_input = LineEdit.new()
	chat_input.placeholder_text = "press Enter to chat…"
	chat_input.size = Vector2(560, 44); chat_input.position = Vector2(20, vp.y - 52)
	chat_input.add_theme_font_size_override("font_size", 20)
	ui.add_child(chat_input)
	chat_input.text_submitted.connect(_on_chat_submit)

	# --- WoW-style action bar (skills 1 / 2 / 3) with radial cooldown sweep ---
	var cd_shader := Shader.new()
	cd_shader.code = "shader_type canvas_item;\nuniform float frac : hint_range(0.0, 1.0) = 0.0;\nvoid fragment() {\n\tvec2 c = UV - vec2(0.5, 0.5);\n\tfloat ang = atan(c.x, -c.y);\n\tif (ang < 0.0) { ang += 6.28318530718; }\n\tfloat a = ang / 6.28318530718;\n\tif (a < frac) { COLOR = vec4(0.0, 0.0, 0.0, 0.62); } else { COLOR = vec4(0.0, 0.0, 0.0, 0.0); }\n}"
	var bar := HBoxContainer.new()
	bar.add_theme_constant_override("separation", 14)
	bar.position = Vector2(vp.x / 2.0 - 145, vp.y - 124)
	ui.add_child(bar)
	skill_ui.clear()
	for i in 3:
		var b := Control.new(); b.custom_minimum_size = Vector2(86, 86)
		bar.add_child(b)
		var frame := ColorRect.new(); frame.color = Color(0, 0, 0, 0.6); frame.size = Vector2(86, 86); b.add_child(frame)
		var bg := ColorRect.new(); bg.color = SKILL_COL[i]; bg.position = Vector2(3, 3); bg.size = Vector2(80, 80); b.add_child(bg)
		var icon := Label.new(); icon.text = SKILL_ICON[i]; icon.add_theme_font_size_override("font_size", 40); icon.position = Vector2(22, 12); b.add_child(icon)
		var ov := ColorRect.new(); ov.color = Color.WHITE; ov.position = Vector2(3, 3); ov.size = Vector2(80, 80)
		ov.mouse_filter = Control.MOUSE_FILTER_IGNORE
		var mat := ShaderMaterial.new(); mat.shader = cd_shader; mat.set_shader_parameter("frac", 0.0); ov.material = mat
		b.add_child(ov)
		var keyl := Label.new(); keyl.text = str(i + 1); keyl.add_theme_font_size_override("font_size", 18); keyl.position = Vector2(7, 2); b.add_child(keyl)
		var namel := Label.new(); namel.text = SKILL_NAMES[i]; namel.add_theme_font_size_override("font_size", 13); namel.position = Vector2(7, 66); b.add_child(namel)
		var cdl := Label.new(); cdl.add_theme_font_size_override("font_size", 36); cdl.position = Vector2(28, 20); cdl.add_theme_color_override("font_color", Color.WHITE); b.add_child(cdl)
		skill_ui.append({"mat": mat, "cdl": cdl})

func _on_chat_submit(t: String) -> void:
	t = t.strip_edges()
	if t != "" and ws.get_ready_state() == WebSocketPeer.STATE_OPEN:
		ws.send_text("say " + t)
	chat_input.text = ""
	chat_input.release_focus()

# ---------- main loop ----------
func _process(delta: float) -> void:
	elapsed += delta
	ws.poll()
	var st := ws.get_ready_state()
	if st == WebSocketPeer.STATE_OPEN:
		if not connected:
			connected = true
			ws.send_text("join " + player_name)
		while ws.get_available_packet_count() > 0:
			_handle(ws.get_packet().get_string_from_utf8())
		_send_input()
	_interp(delta)
	_update_camera(delta)
	_update_ui()
	if capture_mode:
		_drive_capture()

func _send_input() -> void:
	if capture_mode:
		return
	if chat_input.has_focus():
		if last_keys != "0 0 0 0":
			last_keys = "0 0 0 0"; ws.send_text("keys 0 0 0 0")
		return
	var w := int(Input.is_key_pressed(KEY_W) or Input.is_key_pressed(KEY_UP))
	var s := int(Input.is_key_pressed(KEY_S) or Input.is_key_pressed(KEY_DOWN))
	var a := int(Input.is_key_pressed(KEY_A) or Input.is_key_pressed(KEY_LEFT))
	var d := int(Input.is_key_pressed(KEY_D) or Input.is_key_pressed(KEY_RIGHT))
	var keys := "%d %d %d %d" % [w, s, a, d]
	if keys != last_keys:
		last_keys = keys
		ws.send_text("keys " + keys)

func _input(event: InputEvent) -> void:
	if capture_mode or chat_input.has_focus():
		return
	if event is InputEventKey and event.pressed and not event.echo:
		match event.keycode:
			KEY_SPACE, KEY_J:
				if ws.get_ready_state() == WebSocketPeer.STATE_OPEN: ws.send_text("attack")
			KEY_E:
				if ws.get_ready_state() == WebSocketPeer.STATE_OPEN: ws.send_text("interact")
			KEY_1:
				if ws.get_ready_state() == WebSocketPeer.STATE_OPEN: ws.send_text("skill 0")
			KEY_2:
				if ws.get_ready_state() == WebSocketPeer.STATE_OPEN: ws.send_text("skill 1")
			KEY_3:
				if ws.get_ready_state() == WebSocketPeer.STATE_OPEN: ws.send_text("skill 2")
			KEY_ENTER, KEY_KP_ENTER:
				chat_input.grab_focus()

func _handle(msg: String) -> void:
	for line in msg.split("\n", false):
		var p := line.split("\t")
		match p[0]:
			"w":
				my_id = int(p[1])
			"t":
				_msg_time = float(p[1])
			"p":
				var id := int(p[1])
				if not players.has(id):
					players[id] = _make_player(id, p[10])
				var e: Dictionary = players[id]
				e.tx = float(p[2]); e.tz = float(p[3]); e.tyaw = float(p[4])
				if id == my_id:
					hp = float(p[5]); quest = int(p[7]); prog = int(p[8]); kills = int(p[9])
					if p.size() >= 14:
						skill_cd = [float(p[11]), float(p[12]), float(p[13])]
			"m":
				var id := int(p[1])
				if not wolves.has(id):
					wolves[id] = _make_wolf(id)
				var e: Dictionary = wolves[id]
				e.tx = float(p[2]); e.tz = float(p[3]); e.tyaw = float(p[4]); e.state = int(p[5])
				e.hp = float(p[6])
				e.root.visible = e.state != 2
				var show_hp: bool = e.hp < 0.99 and e.state != 2
				e.hpbg.visible = show_hp
				e.hpfg.visible = show_hp
				e.hpfg.scale.x = max(0.04, e.hp)
			"x":
				if _msg_time != _last_evt_time:
					_last_evt_time = _msg_time
					for ev in p[1].split(";", false):
						var f := ev.split(":")
						if f.size() < 4: continue
						_play_event(f[0], int(f[1]), float(f[2]), float(f[3]))
			"n":
				var idx := int(p[1])
				if not npcs.has(idx):
					npcs[idx] = _make_npc(idx, p[2], float(p[3]), float(p[4]), p[5] == "1")
			"c":
				chat_lines.append("[b]%s[/b]: %s" % [p[1], p[2]])
				if chat_lines.size() > 8: chat_lines.pop_front()
				chat_log.text = "\n".join(chat_lines)

func _play_event(kind: String, id: int, x: float, z: float) -> void:
	match kind:
		"s":
			if players.has(id):
				players[id].swing = 0.28          # trigger a sword swing
		"h":
			_spark(Vector3(x, 0.9, z), Color(1, 0.85, 0.2), 6)
			if wolves.has(id):
				var r: Node3D = wolves[id].root   # flinch
				r.scale = Vector3.ONE * 1.28
				create_tween().tween_property(r, "scale", Vector3.ONE, 0.16)
		"k":
			_spark(Vector3(x, 0.7, z), Color(0.7, 0.25, 0.2), 16)
		"C": # Cleave — expanding ground ring
			_ring(Vector3(x, 0.25, z), Color(1, 0.6, 0.15), 4.6)
		"F": # Fireball — bolt flies from caster to target, then bursts
			var from := Vector3(x, 1.0, z)
			if players.has(id):
				from = players[id].root.position + Vector3(0, 1.0, 0)
			_fireball(from, Vector3(x, 1.0, z))
		"L": # Heal — green sparkle on the caster
			_spark(Vector3(x, 1.2, z), Color(0.3, 1.0, 0.45), 16)

func _ring(pos: Vector3, color: Color, r: float) -> void:
	var ring := MeshInstance3D.new()
	var tm := TorusMesh.new(); tm.inner_radius = 0.5; tm.outer_radius = 0.8; ring.mesh = tm
	var m := _mat(color); m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.emission_enabled = true; m.emission = color
	ring.material_override = m; ring.position = pos; add_child(ring)
	var tw := create_tween().set_parallel(true)
	tw.tween_property(ring, "scale", Vector3(r, 1.0, r), 0.35)
	tw.tween_property(m, "albedo_color:a", 0.0, 0.35)
	tw.set_parallel(false); tw.tween_callback(ring.queue_free)

func _fireball(from: Vector3, to: Vector3) -> void:
	var fb := MeshInstance3D.new()
	var sm := SphereMesh.new(); sm.radius = 0.35; sm.height = 0.7; fb.mesh = sm
	var m := _mat(Color(1, 0.5, 0.12)); m.emission_enabled = true; m.emission = Color(1, 0.5, 0.12)
	fb.material_override = m; fb.position = from; add_child(fb)
	var tw := create_tween()
	tw.tween_property(fb, "position", to, 0.22)
	tw.tween_callback(fb.queue_free)
	var tw2 := create_tween()
	tw2.tween_interval(0.22)
	tw2.tween_callback(_spark.bind(to, Color(1, 0.6, 0.2), 12))

func _spark(pos: Vector3, color: Color, n: int) -> void:
	for i in n:
		var m := MeshInstance3D.new()
		var bm := BoxMesh.new(); bm.size = Vector3(0.16, 0.16, 0.16); m.mesh = bm
		m.material_override = _mat(color)
		m.position = pos
		add_child(m)
		var dir := Vector3(randf() - 0.5, randf() * 0.9, randf() - 0.5).normalized() * (1.0 + randf() * 1.6)
		var tw := create_tween().set_parallel(true)
		tw.tween_property(m, "position", pos + dir, 0.4)
		tw.tween_property(m, "scale", Vector3.ZERO, 0.4)
		tw.set_parallel(false)
		tw.tween_callback(m.queue_free)

func _interp(delta: float) -> void:
	var t: float = clamp(delta * 12.0, 0.0, 1.0)
	for e in players.values():
		var r: Node3D = e.root
		r.position = r.position.lerp(Vector3(e.tx, 0, e.tz), t)
		r.rotation.y = lerp_angle(r.rotation.y, e.tyaw, t)
		if e.swing > 0.0:
			e.swing -= delta
			var ph: float = 1.0 - clamp(e.swing / 0.28, 0.0, 1.0)
			e.arm.rotation.x = -2.6 * sin(ph * PI)
		elif e.arm.rotation.x != 0.0:
			e.arm.rotation.x = 0.0
	for e in wolves.values():
		var r: Node3D = e.root
		r.position = r.position.lerp(Vector3(e.tx, 0, e.tz), t)
		r.rotation.y = lerp_angle(r.rotation.y, e.tyaw, t)

func _update_camera(delta: float) -> void:
	var target := Vector3(0, 0, 7)
	if players.has(my_id):
		target = players[my_id].root.position
	var want := target + Vector3(0, 11, 14)
	cam.position = cam.position.lerp(want, clamp(delta * 5.0, 0, 1)) if cam.position != Vector3.ZERO else want
	cam.look_at(target + Vector3(0, 1.2, 0), Vector3.UP)

func _update_ui() -> void:
	status_label.text = "HP %d   ☠ %d wolves slain" % [int(hp), kills]
	match quest:
		0, 3:
			quest_label.text = "Quest: find Mayor Bram (the ! in town) and press E"
		1:
			quest_label.text = "Quest — Cull the Wolves: %d/5 slain" % prog
		2:
			quest_label.text = "Quest: return to Mayor Bram (press E) to turn in!"
	for i in skill_ui.size():
		var rem: float = skill_cd[i]
		skill_ui[i].mat.set_shader_parameter("frac", clampf(rem / SKILL_MAX[i], 0.0, 1.0))
		skill_ui[i].cdl.text = str(int(ceil(rem))) if rem > 0.05 else ""

# auto-drive a quick scene for the headless screenshot
func _drive_capture() -> void:
	if ws.get_ready_state() != WebSocketPeer.STATE_OPEN:
		return
	if elapsed < 1.7:
		ws.send_text("keys 1 0 0 0") # walk north toward the Mayor
	elif not did_interact:
		did_interact = true
		ws.send_text("keys 0 0 0 0")
		ws.send_text("interact")     # accept the quest
		ws.send_text("attack")
	if elapsed > 2.3 and not _s2: _s2 = true; ws.send_text("skill 2") # Heal (long cd)
	if elapsed > 2.7 and not _s1: _s1 = true; ws.send_text("skill 1") # Fireball
	if elapsed > 5.05 and not _s0: _s0 = true; ws.send_text("skill 0") # Cleave (ring shows in shot)
	if elapsed > 5.5 and not captured:
		captured = true
		_capture()

func _capture() -> void:
	await RenderingServer.frame_post_draw
	var img := get_viewport().get_texture().get_image()
	if img != null:
		img.save_png(capture_path)
	get_tree().quit()
