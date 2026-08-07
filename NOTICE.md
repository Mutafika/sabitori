# NOTICE

Sabitori is licensed under the MIT License (see `LICENSE`).

## Third-party references

- **wgpu / winit / cosmic-text / taffy / etagere** — direct dependencies
  under MIT or MIT/Apache-2.0 dual licenses. See `Cargo.toml`.
- **Signed Distance Field rounded rectangle** — the SDF formula used in
  `shaders/rect.wgsl` is a standard technique; original reference:
  Inigo Quilez, https://iquilezles.org/articles/distfunctions2d/
- **REC.601 luma weights (0.299 / 0.587 / 0.114)** used in
  `shaders/glyph.wgsl` are from the ITU-R BT.601 standard.
