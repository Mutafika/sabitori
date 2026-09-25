# Sabitori Roadmap

**Languages**: English · [日本語](ROADMAP.ja.md)

## Current Status

`0.22.0` (pre-release). The core feature set is in place; the WASM target is buildable via the `templates/wasm/` setup. Consumers are expected to depend on a **git tag** — the crates depend on each other by path and `include_str!` their shaders from outside the crate.

The current focus is closing the gaps that showed up writing a real line-of-business app (a rental/reservation system) on sabitori; "What line-of-business apps and the web still need" below is that list, and every entry has an issue. Releases ship from a single line — now `0.12.x`. See [CHANGELOG.md](./CHANGELOG.md) for what landed in each version.

## Implemented

### Rendering & Layout
- ✅ wgpu-based GPU renderer (SDF rounded rect + shadow + gradient + border + rotation)
- ✅ cosmic-text integration + glyph atlas (subpixel alignment, gamma-corrected contrast)
- ✅ Image texture rendering (async URL loading + cache)
- ✅ 3D scene rendering (`scene3d.wgsl` + `OrbitCamera`)
- ✅ Flexbox / Grid layout via Taffy
- ✅ Overflow scrolling (inertia + bounce + 2D)

### Declarative API
- ✅ `DeclarativeApp` trait + `Element` builders (`div() / text() / button() / image()`)
- ✅ `ViewContext` (hovered / focused / scroll_info / image_url loading)
- ✅ ID-based `on_click` routing
- ✅ `EmbeddedRunner` (run sabitori embedded outside winit)

### Input
- ✅ Pointer abstraction (mouse / touch / pen unified)
- ✅ Japanese IME + preedit composition
- ✅ Tab / Shift+Tab focus traversal
- ✅ Pinch gestures, inertia scrolling, bounce
- ✅ macOS native drag & drop (file drop)

### Animation
- ✅ Spring physics (`snappy` / `gentle` / `bouncy` presets)
- ✅ 11 easings + custom cubic Bezier
- ✅ Keyframes + RepeatMode (Once / Loop / PingPong)
- ✅ Specialized states: Typewriter / Spinner / ProgressBar / Gradient / Wave / Pulse / ColorCycle
- ✅ 10 splash presets
- ✅ Presence enter/exit + StyleAnimator (auto interpolation for fill / border / text)

### Widgets (`sabitori-widgets`, 20)
Button / TextInput / Slider / Dropdown / Modal / Card / Panel /
ScrollView / Table (virtual scroll + sort) / Tabs / TreeView / VirtualList /
SplitPane / Tooltip / Toast / ContextMenu / FileBrowser / DragManager /
StyleAnimator / PresenceAnimator

### TUI Components (`sabitori-core::tui`)
- ✅ Block (titled box) / Separator / StatusBar / KeyHint
- ✅ Gradient text / Wave text
- ✅ ANSI 16-color + xterm-256 palette

### Style
- ✅ CSS-like `StyleProps` (margin / padding / flex / position / overflow / z-index)
- ✅ Gradient fills via `Fill::LinearGradient`
- ✅ `BoxShadow` (offset / blur / spread / color)
- ✅ Theme system (YAML loading + opacity)

### Markdown
- ✅ `sabitori-markdown`: CommonMark + GFM (tables / strikethrough / footnotes)
- ✅ TOC extraction, image resolver hook

### Network
- ✅ `sabitori-net::fetch_bytes`: cfg-split between reqwest (native) and fetch API (wasm)
- ✅ Async image loading + decode

### WASM / Cross-platform
- ✅ wasm-bindgen + WebGL2 fallback (auto-detect WebGPU)
- ✅ Trunk build template (see `templates/wasm/`)
- ✅ Canvas auto-binding via winit's web extension
- ✅ Lazy render mode (pauses the 60fps idle loop)

## Planned / Not Yet Started

### macOS Native Integration
`objc2-app-kit` is already a dependency, but only drag & drop is wired up.

- ⬜ NSStatusItem (menu bar resident icon)
- ⬜ Transparent NSWindow + wgpu rendering (overlay use cases)
- ⬜ macOS notifications (UNUserNotificationCenter)
- ⬜ launchd daemon sample

### Physical-unit Layout
- ⬜ `Mm(f32)` / `Pt(f32)` types
- ⬜ PPI detection via OS APIs (finer than winit's `scale_factor`)
- ⬜ GPU capability detection → automatic quality tier (currently a manual `QualityPreset`)

### crates.io Publishing
- ⬜ Per-crate `description` / `keywords` / `categories` / `readme` metadata
- ⬜ `version = "..."` on inter-crate dependencies
- ⬜ `release-plz` setup for lockstep release automation
- ⬜ `#[doc]` comments for docs.rs

### What line-of-business apps and the web still need

Gaps found by writing a real reservation-management app (~20 screens) on sabitori.
Nothing here is a commitment yet — but **every entry has an issue, with the
workaround the app is using written down in it**, so consumers can decide
whether to wait or write their own.

- ⬜ Japanese IME / soft keyboard on the web (hidden-textarea bridge) — [#73](https://github.com/Mutafika/sabitori/issues/73)
- ⬜ Clipboard on the web (via `copy` / `paste` events) — [#76](https://github.com/Mutafika/sabitori/issues/76)
- ⬜ URL / back button (History) on the web, and driving horizontal scroll from the app — [#74](https://github.com/Mutafika/sabitori/issues/74)
- ⬜ Picking and saving files (native dialogs / web input + download) — [#77](https://github.com/Mutafika/sabitori/issues/77)
- ⬜ An HTTP client (POST/PUT/DELETE, JSON, cookies) shared by native and wasm — [#63](https://github.com/Mutafika/sabitori/issues/63)
- ⬜ A standard way to get async results back into the UI (`Tasks`) — [#64](https://github.com/Mutafika/sabitori/issues/64)
- ⬜ A light theme, and widget default styles that follow `AppTheme` — [#65](https://github.com/Mutafika/sabitori/issues/65)
- ⬜ Forms: password fields [#61](https://github.com/Mutafika/sabitori/issues/61), disabled state [#62](https://github.com/Mutafika/sabitori/issues/62)
- ⬜ Small parts (time picker, sticky, Elements in table cells, …) — [#75](https://github.com/Mutafika/sabitori/issues/75)
- ⬜ Charts / printing / off-screen rendering / i18n — [#75](https://github.com/Mutafika/sabitori/issues/75) items 11–12, plus separate calls

### Under Consideration
- ⬜ WebSocket / SSE client
- ✅ Rust code hot reload (subsecond / `feature = "hot-reload"`)
- ⬜ Custom shader hot reload
- ⬜ Dedicated CSS Grid style props (currently passed through to Taffy)
