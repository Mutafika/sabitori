# Sabitori

**Languages**: English · [日本語](README.ja.md)

> A Rust GPU UI framework — wgpu + Taffy + cosmic-text. Declarative API targeting both desktop and WASM.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](Cargo.toml)

**Status**: pre-release (`0.1.0`). Core features are implemented and the WASM target is buildable via the `templates/wasm/` setup. The API is still in flux.

## Features

- **Declarative builder API** — compose trees with `div() / text() / button() / image()`
- **GPU rendering** — wgpu + SDF shaders for rounded corners, borders, shadows, and gradients in a single pass
- **WASM-first** — WebGPU preferred with WebGL2 fallback; canvas auto-binding via winit's web extension
- **Spring physics animation** — snappy / gentle / bouncy presets + 11 easing functions + keyframes
- **Unified input** — mouse / touch / pen abstracted as `Pointer`; Japanese IME and preedit support
- **20 widgets** — Modal / Table (virtual scroll) / TreeView / SplitPane / ContextMenu and more
- **Markdown renderer** — CommonMark + GFM (tables / strikethrough / footnotes) + TOC
- **TUI components** — Block / StatusBar / Spinner / Typewriter for terminal-style UIs
- **MIT purity** — `cargo deny` automatically rejects AGPL/GPL dependencies

## Quick Start

```rust
use sabitori::*;
use sabitori::element::*;

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
cargo run --example filer         # File manager (Table / ContextMenu / rename)
cargo run --example hello         # Low-level API (`SabitoriApp` trait)
```

## Architecture

A 13-crate workspace:

```
sabitori (umbrella)
├── sabitori-core      Element builders / core types / TUI components
├── sabitori-gpu       wgpu SDF renderer / OrbitCamera / image textures
├── sabitori-style     CSS-like StyleProps / Theme / ANSI palette
├── sabitori-layout    Taffy wrapper (Flexbox + Grid)
├── sabitori-scene     NodeTree / hit test / state management
├── sabitori-input     Pointer abstraction / IME / focus
├── sabitori-anim      Spring / Easing / Keyframe / specialized states
├── sabitori-text      cosmic-text integration / glyph atlas
├── sabitori-widgets   20 high-level widgets
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
- macOS native integration (NSStatusItem / transparent NSWindow / notifications)
- Physical units (`Mm` / `Pt`) and accurate PPI detection
- crates.io publishing prep (metadata cleanup + `release-plz` automation)

## License

[MIT](LICENSE). For third-party licenses and references to standard techniques used, see [NOTICE.md](NOTICE.md).

## Requirements

- Rust 1.85+ (edition 2024)
- A wgpu 24-compatible GPU backend (Vulkan / Metal / DX12 / WebGPU / WebGL2)
