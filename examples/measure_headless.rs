//! Measure text with no window, no adapter, no GPU.
//!
//! `TextRenderer::new` needs a `wgpu::Device`, which a headless tool has no way
//! to produce — a DXF importer, a paper-layout pass or a PDF writer runs long
//! before any surface exists. [`TextShaper`] is the same font stack without
//! that requirement, so the numbers here are the numbers sabitori will draw
//! with.
//!
//! ```sh
//! cargo run --example measure_headless
//! ```
//!
//! The baseline is the point of it. sabitori places the top of the **line box**
//! at an element's position, and that box is `line_height` tall (1.4em by
//! default), so the baseline sits lower than the 1.0em that DXF/CAD conventions
//! assume. Printing both makes the offset a number you can subtract instead of
//! a discrepancy you discover on paper.

use sabitori::TextShaper;
use sabitori_core::Typography;

fn main() {
    let mut shaper = TextShaper::new();
    println!("locale: {}", shaper.font_system.locale());
    println!();

    let em = 100.0_f32;
    let typo = Typography::default();

    println!("font size (em) = {em}px, line_height = {}x", 1.4);
    println!(
        "{:<24} {:>9} {:>9} {:>10} {:>10}",
        "text", "width", "height", "baseline", "baseline/em"
    );
    println!("{}", "-".repeat(66));

    for text in ["室名", "R-101", "8000", "Room Name", "室名 R-101"] {
        let m = shaper.measure_text(text, em, false, false, None, None, None, typo);
        println!(
            "{:<24} {:>9.2} {:>9.2} {:>10.2} {:>10.3}",
            text,
            m.size.width,
            m.size.height,
            m.baseline,
            m.baseline / em
        );
    }

    println!();
    // The conversion a CAD host actually needs: DXF's TOP anchor is exactly
    // 1.0em above the baseline, sabitori's is the line-box top. The gap is
    // whatever the shaper reports minus one em.
    //
    // Note this is per-string, not a constant: the two rows below differ
    // because CJK and Latin resolve through faces with different ascents.
    // A single hard-coded offset is wrong for one of them.
    for text in ["室名", "R-101"] {
        let m = shaper.measure_text(text, em, false, false, None, None, None, typo);
        println!(
            "DXF TOP → sabitori pos for {text:<8}: shift down {:.2}px ({:.3}em)",
            m.baseline - em,
            (m.baseline - em) / em
        );
    }

    println!();
    println!("per-character advance (measured, not assumed):");
    for (label, text, n) in [("full-width CJK", "室名室名", 4.0), ("half-width digits", "8000", 4.0)] {
        // `measure_text` pads the box by 2px to dodge sub-pixel truncation.
        let w = shaper
            .measure_text(text, em, false, false, None, None, None, typo)
            .size
            .width
            - 2.0;
        println!("  {label:<18} {:.3}em", w / n / em);
    }
}
