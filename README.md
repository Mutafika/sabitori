# Sabitori

**Languages**: English · [日本語](README.ja.md)

> A Rust GPU UI framework — wgpu + Taffy + cosmic-text. Declarative API targeting both desktop and WASM.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](Cargo.toml)

**Status**: pre-release (`0.4.0`). The core feature set is in place and the WASM target builds via `templates/wasm/`. `0.4.0` is a breaking round that removed the APIs which compiled but silently did nothing — see [CHANGELOG.md](CHANGELOG.md).

## Features

- **Declarative builder API** — compose trees with `div() / text() / button() / image()`
- **CSS-shaped layout** — flexbox *and* grid via Taffy, plus `align-self`, `align-content`, `aspect-ratio`, `z-index`, `text-align`
- **GPU rendering** — wgpu + SDF shaders for rounded corners, borders, shadows, and gradients in a single pass
- **WASM-first** — WebGPU preferred with WebGL2 fallback; canvas auto-binding via winit's web extension
- **Spring physics animation** — snappy / gentle / bouncy presets + 11 easing functions + keyframes
- **Unified input** — mouse / touch / pen abstracted as `Pointer`; Japanese IME with inline preedit and a correctly positioned caret
- **Headless testing** — drive a whole app with no window and no GPU via `sabitori::testing::Harness`
- **Accessibility scaffolding** — `.role()` / `.label()` / `.heading(n)` describe the tree for assistive tech
- **Markdown renderer** — CommonMark + GFM (tables / strikethrough / footnotes) + TOC
- **TUI components** — Block / StatusBar / Spinner / Typewriter for terminal-style UIs
- **MIT purity** — `cargo deny` automatically rejects AGPL/GPL dependencies

## Quick Start

```rust
use sabitori::*;

struct App { clicks: u32 }

impl DeclarativeApp for App {
    fn title(&self) -> &str { "Hello Sabitori" }
    fn size(&self) -> (f32, f32) { (800.0, 600.0) }

    fn view(&self, ctx: &ViewContext) -> Element {
        div()
            .w(Px(ctx.width)).h(Px(ctx.height))
            .bg(Color::from_hex("#1a1b26"))
            .flex_col()
            .items_center()
            .justify_center()
            .gap(16.0)
            .children([
                text(&format!("Clicks: {}", self.clicks))
                    .font_size(24.0)
                    .color(Color::from_hex("#c0caf5")),
                button("Click Me")
                    .id("btn")
                    .accent(Color::from_hex("#7aa2f7")),
            ])
    }

    fn on_click(&mut self, id: &str) {
        if id == "btn" { self.clicks += 1; }
    }
}

fn main() {
    sabitori::run_declarative(App { clicks: 0 });
}
```

Anything you want to interact with needs an id — that is what click, hover, focus, and scroll are keyed on.

**Write what a click does next to the element that gets clicked:**

```rust
div().click(ctx, "save", |app: &mut App| app.saved = true)
```

`click` assigns the id and registers the handler in one call, so there is no second place to keep in sync. For a list, capture the index instead of parsing it back out of the id:

```rust
div().click(ctx, format!("row-{i}"), move |app: &mut App| app.selected = Some(i))
```

The older form — `.id("save")` plus a `fn on_click(&mut self, id: &str)` that matches on strings — still works and is still there for dynamic dispatch. But a typo in either string compiles fine and silently does nothing, so prefer `click`.

## The four things people get wrong

These come up more than everything else combined. Each has exactly one correct form.

### 1. Scrolling

**Give the container `.scroll(id)` and let the runtime own the offset.**

```rust
div().scroll("file-list").flex_1().flex_col().children(rows)
```

That is the whole wiring. The runtime routes the wheel, runs the momentum spring, and keeps the position across frames. Do **not** implement `on_scroll` for it — the wheel is already delivered, so adding your own handler scrolls twice.

Read the position back with `ctx.scroll_info("file-list")`, and scroll programmatically by returning from `scroll_intents()`:

```rust
fn scroll_intents(&mut self) -> Vec<(String, f32)> {
    self.pending.take().map(|y| ("file-list".into(), y)).into_iter().collect()
}
```

For long lists, ask the runtime which rows are visible and pad the rest with spacers so the scrollbar length matches the real data:

```rust
let (first, count) = ctx.visible_range("file-list", ROW_H);
```

`virtual_list(ctx, id, &items, row_h, render)` does that for you.

The other model is `.scroll_manual(x, y)`, where **your app** owns the offset and the runtime never touches it. Pick one; the type says which.

### 2. Text input and the IME

**Put `text_input` in `view()`. That is the entire wiring.**

```rust
struct App { name: TextInputState, saved: Option<String> }

impl DeclarativeApp for App {
    fn view(&self, ctx: &ViewContext) -> Element {
        text_input(ctx, "name", &self.name, &TextInputStyle::default_dark())
    }
    fn on_click(&mut self, id: &str) {
        if id == "save" { self.saved = Some(self.name.text()); }
    }
}
```

Nothing else. Keystrokes, IME composition, paste, caret blink, focus state, and the position of the OS candidate window are all handled by the runtime, because the widget registers itself with the `ViewContext` when it builds. There is no `on_focused_input`, no `tick`, no `ime_cursor_area` to forget.

Japanese conversion shows inline with the caret **inside** the preedit, which is how you can tell what is being converted.

Read and write the value through accessors — `text()`, `set_text()`, `clear()`, `is_focused()`, `is_composing()`. The state is a cheap-to-clone shared handle, which is what lets `view(&self)` hand it to the runtime.

If you hand-roll a text field instead (your own element declaring `Role::TextInput`), wiring is back on you — and the runtime will warn, once, when typed characters reach nothing:

```rust
assert!(h.unrouted_text_inputs().is_empty());
```


For a wrapping, multi-line field use `text_area` — same state type, same zero wiring:

```rust
text_area(ctx, "memo", &self.memo, &TextInputStyle::default_dark(), 6)  // 6 lines tall
```

| | `text_input` | `text_area` |
|---|---|---|
| Enter | bubbles to your app (form submit) | inserts a newline |
| Paste | newlines collapse to spaces | newlines preserved |
| ↑ ↓ | bubble to your app | move one **visual** line |
| Home / End | ends of the string | ends of the **visual** line |

"Visual line" is the point: moving by logical line (`\n`) makes one keypress jump a whole wrapped paragraph. `Cmd+Enter` still bubbles out, so you can bind it to "send".

### 3. Focus and keyboard

Elements with `.focusable` take focus on click and via Tab. Keys go to `on_focused_input(id, event)` first; whatever is unhandled falls through to `on_input(event)`.

**`on_input` returning `true` suppresses the built-in behavior** for that key (copy, paste, Escape, Tab). Return `false` when you did not consume it, or you will silently kill the defaults.

### 4. Testing

Apps are testable with no window and no GPU:

```rust
use sabitori::testing::Harness;

let mut h = Harness::new(App::default(), 800.0, 600.0);
h.frame();                  // build + layout
h.click("name");            // focus the field by id
h.text("hello");            // typed input goes to the focused element
h.click("save");            // now the handler sees the typed value
h.scroll("file-list", 400.0);
h.settle();                 // let springs finish (needed for scroll_intents)
assert_eq!(h.app().saved.as_deref(), Some("hello"));
```

`frame()` does not advance time. Anything spring-driven — momentum scroll, `scroll_intents`, style animation — needs `tick(dt)` or `settle()`.

## Layout

Flexbox and grid, both backed by Taffy. If you know CSS you already know this — the names match.

```rust
// Flex
let toolbar = div().flex_row().items_center().justify_between().gap(8.0);

// Grid — a fixed sidebar and a body that takes the rest
let shell = grid()
    .grid_cols([Track::px(240.0), Track::fr(1.0)])
    .gap(12.0)
    .children([sidebar, body]);

// A header spanning every column
let sheet = grid()
    .grid_cols(Track::repeat(3, Track::fr(1.0)))
    .children([header.col_span(3), a, b, c]);
```

`Track` is CSS `minmax(min, max)`: build one with `Track::px / pct / fr / auto / min_content / max_content / minmax`, and repeat it with `Track::repeat(n, track)`. `auto-fill` / `auto-fit` are not implemented — you pick the count.

| | |
|---|---|
| One child opting out of the parent's alignment | `.self_start()` `.self_center()` `.self_end()` `.self_stretch()` |
| Distributing wrapped **lines** | `.align_content(..)` on a `.wrap()` container |
| Locking width-to-height | `.aspect(16.0 / 9.0)` |
| Stacking order among siblings | `.z(5)` |
| Aligning wrapped text | `.text_center()` `.text_right()` |

Three of these have a precondition worth knowing before you decide they are broken:

- **`.aspect()` loses to stretch.** In a `flex_col` the default `align_items: stretch` already fixes the child's width; with two sides determined there is nothing for the ratio to decide. Add `.self_start()` when you want height to drive width.
- **`.text_center()` needs a width.** A text element sizes to its content, and content-sized boxes have no slack to align within. Inside a `flex_col` it stretches to the parent and just works; inside a `flex_row` it will not.
- **`.z()` does not escape the parent**, exactly like a CSS stacking context. It reorders siblings — paint order *and* click order together. To lift something above the whole tree (popups, context menus) use `.overlay()`.

`display: none` has no equivalent on purpose: don't emit the element. That removes its layout cost too.

## Widgets

Two kinds, and the split is the API:

- **State** is a struct you keep on your app: `TextInputState`, `TableState`, `DropdownState`, `SplitPaneState`.
- **Visuals** are free functions you call from `view()`: `text_input(ctx, id, &state, &style) -> Element`.

Every Element-producing entry point is a `snake_case` free function taking `&ViewContext` first and `id` second. `sabitori_core::forms` (`checkbox`, `radio`, `slider`, `segment_control`, `progress_bar`, `numeric_input`, `collapsing_header`, `dropdown_trigger`) follows the same shape, so there is nothing to look up per widget.

```rust
div().flex_col().children([
    text_input(ctx, "name", &self.name, &TextInputStyle::default_dark()),
    table(ctx, "files", &self.files, &TableStyle::default_dark()),
    tree_view(ctx, "tree", &self.tree, &TreeViewStyle::default_dark()),
])
```

## Accessibility

The window is a GPU surface, so screen readers see nothing unless the tree says what things are. `button()` declares `Role::Button` on its own; describe the rest:

```rust
let close = div().id("close").role(Role::Button).label("Close");   // icon-only button
let heading = text("Settings").role(Role::Heading).heading(2);
```

The semantic layer is in place and carried through `hit_regions`. The OS adapter (accesskit) is not wired yet.

## Examples

```bash
cargo run --example declarative   # Declarative API + hover + click
cargo run --example anim          # Spring animation + mouse follow
cargo run --example effects       # 4 GPU effects (spotlight / magnetic / gravity / fluid)
cargo run --example gpu_flex      # 10,000-particle physics simulation
cargo run --example showcase      # 30-demo grid + modal zoom
cargo run --example layout        # Taffy Flexbox layout
cargo run --example text          # cosmic-text integration
cargo run --example tui_demo      # ANSI-based TUI dashboard
cargo run --example tui_gallery   # Animation gallery
cargo run --example filer         # File manager — runtime-managed scroll, virtualized rows
cargo run --example hello         # Low-level API (`SabitoriApp` trait)
```

## Architecture

A 13-crate workspace:

```
sabitori (umbrella)
├── sabitori-core      Element builders / core types / form controls / TUI components
├── sabitori-gpu       wgpu SDF renderer / OrbitCamera / image textures
├── sabitori-style     Theme / ANSI palette / StyleProps (layout types re-exported from core)
├── sabitori-layout    Taffy wrapper (Flexbox + Grid)
├── sabitori-scene     NodeTree / hit test / state management
├── sabitori-input     Pointer abstraction / IME / focus / delivery table
├── sabitori-anim      Spring / Easing / Keyframe / specialized states
├── sabitori-text      cosmic-text integration / glyph atlas
├── sabitori-widgets   Stateful widgets (state structs + Element functions)
├── sabitori-window    winit runtime / EmbeddedRunner
├── sabitori-markdown  Markdown → Element conversion
└── sabitori-net       HTTP fetch (reqwest / wasm fetch)
```

## WASM Target

`templates/wasm/` contains a `Trunk.toml` + `index.html` template along with a README covering the common pitfalls.

```bash
# One-time setup
rustup target add wasm32-unknown-unknown
cargo install trunk

# Copy the template into your app
cp templates/wasm/{Trunk.toml,index.html} /path/to/your-app/

# Dev server / production build
trunk serve            # hot reload on localhost:8080
trunk build --release  # optimized assets in dist/
```

For WASM-specific requirements (the `webgl` feature on `wgpu`, bundling fonts, the WebGL2 varying limit, etc.) see [`templates/wasm/README.md`](templates/wasm/README.md).

## Roadmap

See [ROADMAP.md](ROADMAP.md) for implemented features and outstanding areas.

Notable open items:
- accesskit adapter so the semantic layer reaches VoiceOver / NVDA / Narrator
- macOS native integration (NSStatusItem / transparent NSWindow / notifications)
- Physical units (`Mm` / `Pt`) and accurate PPI detection
- crates.io publishing prep (metadata cleanup + `release-plz` automation)

## License

[MIT](LICENSE). For third-party licenses and references to standard techniques used, see [NOTICE.md](NOTICE.md).

## Requirements

- Rust 1.85+ (edition 2024)
- A wgpu 24-compatible GPU backend (Vulkan / Metal / DX12 / WebGPU / WebGL2)
