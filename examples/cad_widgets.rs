/// CAD widgets demo — the Phase-0/2c widget set for hosting a CAD app:
///
/// * MenuBar (G1): File / Edit / View with dropdown, hover-to-switch,
///   one-level submenu, shortcuts, separators.
/// * NumericInput (G2): drag horizontally to change the value, click to
///   type (Enter commits, Escape cancels). min/max/step/suffix.
/// * checkbox (G3) and collapsing_section (G4) form builders.
/// * A `scroll(id) + flex_1()` list — no explicit height (G5 fix).
/// * FocusManager: two text fields with click-to-focus, Tab cycling,
///   Enter submit, IME routing.
/// * ColorPickerState: palette grid + RGB numeric fine-tuning.
/// * DropdownState: element-id driven ComboBox (inline menu mode).
/// * DatePickerState: ◀▶ month nav + calendar grid.
/// * SliderState + labeled_slider, and labeled_progress_bar bound to it.
///
/// Run: `cargo run --example cad_widgets`

use sabitori::*;
use std::collections::HashSet;

/// Fixed slider-track geometry (the demo lays the slider out with fixed
/// widths: panel pad 12 + label 70 + gap 8). Embedded hosts should use
/// `sabitori::slider_sync` + `BuildResult::region_rect` instead.
const SLD_TRACK_X: f32 = 12.0 + 70.0 + 8.0;
const SLD_TRACK_W: f32 = 120.0;

struct CadDemo {
    menus: Vec<MenuDef>,
    menu_bar: MenuBarState,
    menu_style: MenuBarStyle,
    last_action: String,

    wall_height: NumericInputState,
    wall_thickness: NumericInputState,

    show_grid: bool,
    open_sections: HashSet<String>,

    // Phase-2c widgets
    focus: FocusManager,
    picker: ColorPickerState,
    structure: DropdownState,
    start_date: DatePickerState,
    opacity: SliderState,

    mouse_x: f32,
}

impl CadDemo {
    fn new() -> Self {
        let menus = vec![
            MenuDef::new(
                "file",
                "File",
                vec![
                    MenuItemDef::action("new", "新規プロジェクト").with_shortcut("Cmd+N"),
                    MenuItemDef::action("open", "開く…").with_shortcut("Cmd+O"),
                    MenuItemDef::separator(),
                    MenuItemDef::action("export", "エクスポート").with_submenu(vec![
                        MenuItemDef::action("export-ifc", "IFC…"),
                        MenuItemDef::action("export-dxf", "DXF…"),
                        MenuItemDef::action("export-ply", "PLY…").disabled(),
                    ]),
                    MenuItemDef::separator(),
                    MenuItemDef::action("quit", "終了").with_shortcut("Cmd+Q"),
                ],
            ),
            MenuDef::new(
                "edit",
                "Edit",
                vec![
                    MenuItemDef::action("undo", "元に戻す").with_shortcut("Cmd+Z"),
                    MenuItemDef::action("redo", "やり直す").with_shortcut("Shift+Cmd+Z").disabled(),
                ],
            ),
            MenuDef::new(
                "view",
                "View",
                vec![
                    MenuItemDef::action("fit", "全体表示").with_shortcut("F"),
                    MenuItemDef::action("top", "上面図"),
                ],
            ),
        ];
        let mut open_sections = HashSet::new();
        open_sections.insert("sec-wall".to_string());
        open_sections.insert("sec-meta".to_string());
        let mut focus = FocusManager::new();
        focus.register("fld-name", "名前を入力");
        focus.register("fld-material", "材質を入力");
        Self {
            menus,
            menu_bar: MenuBarState::new(),
            menu_style: MenuBarStyle::default_dark(),
            last_action: "(メニューから選択)".into(),
            wall_height: NumericInputState::new(2400.0)
                .with_range(0.0, 10000.0)
                .with_step(10.0)
                .with_suffix("mm"),
            wall_thickness: NumericInputState::new(120.0)
                .with_range(10.0, 500.0)
                .with_step(1.0)
                .with_precision(1)
                .with_suffix("mm"),
            show_grid: true,
            open_sections,
            focus,
            picker: ColorPickerState::new("pick", Color::from_hex("#40c0ff")),
            structure: DropdownState::new(
                "dd-structure",
                vec!["木造".into(), "鉄骨造".into(), "RC造".into(), "SRC造".into()],
            ),
            start_date: DatePickerState::new("dp", 2026, 6, 10),
            opacity: SliderState::from_ranged(0.7, 0.0, 1.0),
            mouse_x: 0.0,
        }
    }

    fn text_row<'a>(&self, label: &str, id: &str, t: &'a AppTheme) -> Element {
        div()
            .flex_row()
            .items_center()
            .gap(8.0)
            .children([
                text(label).font_size(12.0).color(t.text_secondary).w(Px(70.0)).shrink(0.0),
                div().flex_1().child(form_text_input(
                    id,
                    &self.focus.display_text(id),
                    self.focus.is_placeholder(id),
                    self.focus.is_focused(id), // caret always visible while focused (demo)
                    0.0,
                    self.focus.is_focused(id),
                    t.text_primary,
                    t.text_secondary,
                    t.surface,
                    t.border,
                    t.primary,
                )),
            ])
    }

    fn numeric_row<'a>(
        &self,
        label: &str,
        id: &str,
        state: &NumericInputState,
        t: &'a AppTheme,
    ) -> Element {
        let display = if state.editing {
            state.edit.display_text_with_preedit()
        } else {
            state.display_text()
        };
        div()
            .flex_row()
            .items_center()
            .gap(8.0)
            .children([
                text(label).font_size(12.0).color(t.text_secondary).w(Px(70.0)).shrink(0.0),
                numeric_input(
                    id,
                    &display,
                    state.editing,
                    state.editing, // cursor always visible while editing (demo)
                    t.text_primary,
                    t.surface,
                    t.border,
                    t.primary,
                )
                .w(Px(120.0)),
            ])
    }

    fn route_numeric_click(&mut self, id: &str) -> bool {
        // Commit any in-progress edit when clicking elsewhere.
        let x = self.mouse_x;
        match id {
            "num-height" => {
                if self.wall_thickness.editing { self.wall_thickness.commit_edit(); }
                self.wall_height.on_pointer_down(x);
                true
            }
            "num-thickness" => {
                if self.wall_height.editing { self.wall_height.commit_edit(); }
                self.wall_thickness.on_pointer_down(x);
                true
            }
            _ => {
                if self.wall_height.editing { self.wall_height.commit_edit(); }
                if self.wall_thickness.editing { self.wall_thickness.commit_edit(); }
                false
            }
        }
    }
}

impl DeclarativeApp for CadDemo {
    fn title(&self) -> &str { "Sabitori — CAD Widgets (G1-G5)" }
    fn size(&self) -> (f32, f32) { (900.0, 640.0) }

    fn view(&self, ctx: &ViewContext) -> Element {
        let t = &ctx.theme;
        let hovered = ctx.hovered.as_deref();

        // ── Left: property panel ────────────────────────────────
        let wall_open = self.open_sections.contains("sec-wall");
        let info_open = self.open_sections.contains("sec-info");
        let meta_open = self.open_sections.contains("sec-meta");
        let color_open = self.open_sections.contains("sec-color");
        let date_open = self.open_sections.contains("sec-date");

        // Dropdown row (構造) — inline menu pushes content down while open.
        let dd_style = DropdownStyle::default_dark();
        let mut dd_col = div()
            .flex_1()
            .flex_col()
            .gap(2.0)
            .child(self.structure.trigger(&dd_style, hovered));
        if let Some(menu) = self.structure.menu_inline(hovered, &dd_style) {
            dd_col = dd_col.child(menu);
        }
        let dropdown_row = div()
            .flex_row()
            .items_start()
            .gap(8.0)
            .children([
                text("構造").font_size(12.0).color(t.text_secondary).w(Px(70.0)).shrink(0.0),
                dd_col,
            ]);

        // Slider (透過率) + progress bar bound to its value.
        let opacity = self.opacity.value();
        let slider_row = labeled_slider(
            "sld-opacity",
            "透過率",
            &format!("{:.0}%", opacity * 100.0),
            opacity,
            70.0,
            SLD_TRACK_W,
            36.0,
            t.text_secondary,
            t.border,
            t.primary,
            t.text_primary,
        );
        let fill_row = labeled_progress_bar(
            "占積率",
            &format!("{:.0}%", opacity * 100.0),
            opacity,
            70.0,
            36.0,
            t.text_secondary,
            t.border,
            t.success,
        );

        let props = div()
            .scroll("props-scroll")
            .w(Px(260.0))
            .h_full()
            .bg(t.surface)
            .p(Px(12.0))
            .flex_col()
            .gap(8.0)
            .scrollbar(t.border)
            .children([
                collapsing_section(
                    "sec-wall", "壁プロパティ", wall_open, t.text_primary, t.surface,
                    vec![
                        self.numeric_row("高さ", "num-height", &self.wall_height, t),
                        self.numeric_row("厚さ", "num-thickness", &self.wall_thickness, t),
                        checkbox("chk-grid", "グリッド表示", self.show_grid, t.text_primary, t.primary, t.border),
                    ],
                ),
                collapsing_section(
                    "sec-meta", "名称 (FocusManager)", meta_open, t.text_primary, t.surface,
                    vec![
                        self.text_row("名前", "fld-name", t),
                        self.text_row("材質", "fld-material", t),
                        text("クリックでフォーカス / Tabで巡回 / Enterで確定")
                            .font_size(10.0)
                            .color(t.text_secondary),
                    ],
                ),
                slider_row,
                fill_row,
                dropdown_row,
                collapsing_section(
                    "sec-color", "色 (ColorPicker)", color_open, t.text_primary, t.surface,
                    vec![self.picker.view(hovered, &ColorPickerStyle::default_dark())],
                ),
                collapsing_section(
                    "sec-date", "着工日 (DatePicker)", date_open, t.text_primary, t.surface,
                    vec![
                        text(&format!("選択中: {}", self.start_date.formatted()))
                            .font_size(11.0)
                            .color(t.text_secondary),
                        self.start_date.view(hovered, &DatePickerStyle::default_dark()),
                    ],
                ),
                collapsing_section(
                    "sec-info", "操作メモ", info_open, t.text_primary, t.surface,
                    vec![
                        text("数値の上で横ドラッグ → 増減").font_size(11.0).color(t.text_secondary),
                        text("クリック → 直接入力 (Enter確定/Escキャンセル)").font_size(11.0).color(t.text_secondary),
                    ],
                ),
                text(&format!("最後の操作: {}", self.last_action))
                    .font_size(11.0)
                    .color(t.text_secondary),
            ]);

        // ── Right: flex_1 + scroll(id) list (G5) ───────────
        let rows: Vec<Element> = (0..60)
            .map(|i| {
                div()
                    .w_full()
                    .h(Px(28.0))
                    .px_pad(Px(10.0))
                    .flex_row()
                    .items_center()
                    .bg(if i % 2 == 0 { t.surface } else { Color::TRANSPARENT })
                    .child(
                        text(&format!("要素 {i:02} — 壁 W-{i:03}"))
                            .font_size(12.0)
                            .color(t.text_primary),
                    )
            })
            .collect();
        let element_list = div()
            .flex_1()
            .h_full()
            .flex_col()
            .children([
                div()
                    .w_full()
                    .h(Px(30.0))
                    .px_pad(Px(10.0))
                    .flex_row()
                    .items_center()
                    .child(text("要素一覧 (flex_1 + scroll(id)、明示高さ無し)")
                        .font_size(12.0).bold().color(t.text_primary)),
                div()
                    .scroll("element-scroll")
                    .flex_1()
                    .flex_col()
                    .scrollbar(t.border)
                    .children(rows),
            ]);

        div()
            .w(Px(ctx.width))
            .h(Px(ctx.height))
            .bg(t.bg)
            .flex_col()
            .children([
                // ── G1: menu bar ─────────────────────────────────
                self.menu_bar.bar(&self.menus, hovered, &self.menu_style),
                div().flex_1().flex_row().children([props, element_list]),
            ])
    }

    fn overlay_view(&self, ctx: &ViewContext) -> Option<Element> {
        self.menu_bar.overlay(
            &self.menus,
            ctx.width,
            ctx.height,
            ctx.hovered.as_deref(),
            &self.menu_style,
        )
    }

    fn on_click(&mut self, id: &str) {
        if let Some(action) = self.menu_bar.handle_click(id, &self.menus) {
            self.last_action = action;
            return;
        }
        // Dropdown (構造)
        match self.structure.handle_click(id) {
            DropdownEvent::Selected(i) => {
                self.last_action = format!("構造 = {}", self.structure.items[i]);
                return;
            }
            DropdownEvent::Opened | DropdownEvent::Closed => return,
            DropdownEvent::Ignored => {}
        }
        // DatePicker (着工日)
        if id.starts_with("dp:") {
            if let Some((y, m, d)) = self.start_date.handle_click(id) {
                self.last_action = format!("着工日 = {y:04}-{m:02}-{d:02}");
            }
            return;
        }
        // ColorPicker: palette swatch / RGB channel press
        if let Some(c) = self.picker.handle_click(id) {
            let (r, g, b, _) = c.to_srgb8();
            self.last_action = format!("色 = RGB({r}, {g}, {b})");
            return;
        }
        if self.picker.on_pointer_down(id, self.mouse_x) {
            return; // an RGB channel grabbed the pointer
        }
        // Slider (透過率) — fixed track geometry in this demo; embedded
        // hosts use slider_sync + BuildResult::region_rect instead.
        if id == "sld-opacity" {
            self.opacity.begin_drag(self.mouse_x, SLD_TRACK_X, SLD_TRACK_W);
            return;
        }
        // FocusManager: registered text fields focus, anything else blurs.
        if let FocusChange::Focused(_) = self.focus.handle_press(Some(id)) {
            return;
        }
        if self.route_numeric_click(id) {
            return;
        }
        match id {
            "chk-grid" => self.show_grid = !self.show_grid,
            "sec-wall" | "sec-info" | "sec-meta" | "sec-color" | "sec-date" => {
                if !self.open_sections.remove(id) {
                    self.open_sections.insert(id.to_string());
                }
            }
            _ => {}
        }
    }

    fn on_hover_change(&mut self, id: Option<&str>) {
        self.menu_bar.handle_hover(id, &self.menus);
    }

    fn on_pointer_move(&mut self, x: f32, _y: f32) {
        self.mouse_x = x;
        self.wall_height.on_pointer_move(x);
        self.wall_thickness.on_pointer_move(x);
        self.picker.on_pointer_move(x);
        self.opacity.drag_to(x, SLD_TRACK_X, SLD_TRACK_W);
    }

    fn on_pointer_up(&mut self) {
        self.wall_height.on_pointer_up();
        self.wall_thickness.on_pointer_up();
        // Click on an RGB channel → edit mode; the runner focuses it
        // (the numeric_input element is focusable), so keyboard input
        // arrives via on_focused_input below.
        self.picker.on_pointer_up();
        self.opacity.end_drag();
    }

    /// FocusManager is the source of truth for the text fields — keep
    /// the runner's focus in sync (this is what makes Tab cycling work).
    fn desired_focus(&self) -> Option<String> {
        self.focus.focused_id().map(String::from)
    }

    fn on_focused_input(&mut self, id: &str, event: &InputEvent) -> bool {
        // FocusManager text fields
        if self.focus.contains(id) {
            return match event {
                InputEvent::KeyInput { key, pressed: true, modifiers } => {
                    match self.focus.on_key(*key, *modifiers) {
                        FocusKeyResult::Submit(fid) => {
                            self.last_action = format!("{fid} = {}", self.focus.text(&fid));
                            true
                        }
                        FocusKeyResult::Escape(_) => {
                            self.focus.blur();
                            true
                        }
                        FocusKeyResult::Consumed | FocusKeyResult::Moved(_) => true,
                        _ => false,
                    }
                }
                InputEvent::CharInput(ch) => self.focus.on_char(*ch),
                InputEvent::ImePreedit { text, cursor } => {
                    self.focus.on_ime_preedit(text.clone(), *cursor)
                }
                InputEvent::ImeCommit { text } => self.focus.on_ime_commit(text),
                _ => false,
            };
        }
        // ColorPicker RGB channels
        if id.starts_with("pick:") {
            return match event {
                InputEvent::KeyInput { key, pressed: true, modifiers } => {
                    self.picker.on_key(*key, *modifiers)
                }
                InputEvent::CharInput(ch) => {
                    self.picker.on_char(*ch);
                    true
                }
                _ => false,
            };
        }
        let state = match id {
            "num-height" => &mut self.wall_height,
            "num-thickness" => &mut self.wall_thickness,
            _ => return false,
        };
        match event {
            InputEvent::KeyInput { key, pressed: true, modifiers } => state.on_key(*key, *modifiers),
            InputEvent::CharInput(ch) => {
                state.on_char(*ch);
                true
            }
            InputEvent::ImeCommit { text } => {
                for ch in text.chars() {
                    state.on_char(ch);
                }
                true
            }
            _ => false,
        }
    }
}

fn main() {
    sabitori::run_declarative(CadDemo::new());
}
