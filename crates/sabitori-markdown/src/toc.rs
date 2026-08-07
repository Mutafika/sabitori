/// Heading extracted from a markdown document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    /// Slug used as the heading element's `id` (prefixed with `"heading:"`
    /// by the renderer).
    pub id: String,
    /// Display text.
    pub text: String,
    /// Heading depth, 1 for h1, 6 for h6.
    pub depth: u32,
}
