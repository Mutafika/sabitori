# Sabitori Roadmap

**Languages**: English · [日本語](ROADMAP.ja.md)

## Current Status

`0.4.0` (pre-release). The core feature set is in place; the WASM target is buildable via the `templates/wasm/` setup. `0.4.0` removed the APIs that compiled but silently did nothing, and unified the widget layer on Element-returning free functions. Next focus: the accesskit adapter, then API stabilization and the unimplemented areas (macOS native integration, physical-unit layout).

Releases ship from a single line — now `0.4.x`. The `0.2.x` maintenance branch was merged back into `main` at `v0.3.1` and is no longer maintained. See [CHANGELOG.md](./CHANGELOG.md) for what landed in each version.

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

### Under Consideration
- ⬜ WebSocket / SSE client
- ✅ Rust code hot reload (subsecond / `feature = "hot-reload"`)
- ⬜ Custom shader hot reload
- ⬜ Dedicated CSS Grid style props (currently passed through to Taffy)
