//! Parse and validate every WGSL shader on the CPU.
//!
//! The renderers feed these files to `create_shader_module` at runtime, so a
//! typo or a type error in WGSL only shows up when a real GPU (or a specific
//! backend) gets to it — and on wasm that means a blank canvas with no test
//! covering it. naga is the same front end wgpu uses, so validating here
//! catches the mistake in `cargo test`.

use wgpu::naga;

const SHADERS: &[&str] = &[
    "rect.wgsl",
    "image.wgsl",
    "glyph.wgsl",
    "line.wgsl",
    "arc.wgsl",
    "blur.wgsl",
    "scene3d.wgsl",
];

fn shaders_dir() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../shaders").to_string()
}

#[test]
fn every_shader_parses_and_validates() {
    for name in SHADERS {
        let path = format!("{}/{name}", shaders_dir());
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));

        let module = naga::front::wgsl::parse_str(&src)
            .unwrap_or_else(|e| panic!("{path}: parse failed:\n{}", e.emit_to_string(&src)));

        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{path}: validation failed: {e:?}"));
    }
}

/// Every shader file must be in `SHADERS`, so adding one without validating it
/// fails here instead of at runtime on someone's GPU.
#[test]
fn the_list_covers_every_shader_file() {
    let mut found: Vec<String> = std::fs::read_dir(shaders_dir())
        .expect("shaders/")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".wgsl"))
        .collect();
    found.sort();
    let mut listed: Vec<String> = SHADERS.iter().map(|s| s.to_string()).collect();
    listed.sort();
    assert_eq!(found, listed);
}
