//! Markdown → Sabitori `Element` tree renderer.
//!
//! Parses CommonMark + GFM with [`pulldown-cmark`] and produces a flex-column
//! `Element` whose children are block-level elements (headings, paragraphs,
//! lists, code blocks, blockquotes, etc.). Intended as the baseline article
//! renderer for any Sabitori app (bisquit, phymath-in-sabitori, sekai, …).
//!
//! Inline-level formatting (bold / italic / inline code inside a paragraph)
//! is currently flattened to plain text with a best-effort marker substitution,
//! because Sabitori's `text()` element doesn't yet support mixed inline styles
//! within a single paragraph. Block-level distinctions (headings, code blocks,
//! blockquotes) are preserved.
//!
//! # Example
//! ```no_run
//! use sabitori_markdown::{render_markdown, MarkdownOptions};
//! let opts = MarkdownOptions::default();
//! let element = render_markdown("# Hello\n\nWorld", &opts);
//! ```

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use sabitori_core::element::{div, image as img_element, text, Dimension, Element, ImageData, ObjectFit, Px};
use sabitori_core::Color;

pub use toc::TocEntry;

mod toc;

/// Visual theme for markdown rendering. Colors default to a dark palette.
#[derive(Clone, Debug)]
pub struct MarkdownTheme {
    pub body: Color,
    pub dim: Color,
    pub heading: Color,
    pub link: Color,
    pub code_fg: Color,
    pub code_bg: Color,
    pub quote_bar: Color,
    pub rule: Color,
    pub base_font_size: f32,
    pub code_font_size: f32,
    pub heading_sizes: [f32; 6], // h1..h6
    pub paragraph_gap: f32,
    /// Maximum rendered width for inline images. Preserves aspect ratio.
    pub max_image_width: f32,
}

impl Default for MarkdownTheme {
    fn default() -> Self {
        Self {
            body: Color::from_hex("#e8e2d6"),
            dim: Color::from_hex("#8a8177"),
            heading: Color::from_hex("#f4edd9"),
            link: Color::from_hex("#c28c56"),
            code_fg: Color::from_hex("#e8d8a0"),
            code_bg: Color::from_hex("#1c1916"),
            quote_bar: Color::from_hex("#c28c56"),
            rule: Color::from_hex("#2a2623"),
            base_font_size: 15.0,
            code_font_size: 13.0,
            heading_sizes: [28.0, 22.0, 18.0, 16.0, 15.0, 14.0],
            paragraph_gap: 12.0,
            max_image_width: 680.0,
        }
    }
}

/// Options for rendering.
#[derive(Clone, Debug, Default)]
pub struct MarkdownOptions {
    pub theme: MarkdownTheme,
    /// If true, extracts a table of contents (h1..h6) alongside the tree.
    /// Use [`render_markdown_with_toc`] to receive both.
    pub extract_toc: bool,
    /// Prefix for link element ids. Clicks on links produce ids shaped
    /// `{prefix}{url}` so the host app can dispatch them.
    /// Default: `"md-link:"`.
    pub link_id_prefix: String,
}

/// Image resolver callback. Returns `Some(ImageData)` when the image at `url`
/// is already decoded and ready; returns `None` to render a placeholder
/// (which the host app can later fill by re-rendering once the image is
/// available). The host app is expected to kick off any necessary fetches
/// from inside this callback (e.g., a `request_image` helper).
pub type ImageResolver<'a> = &'a dyn Fn(&str) -> Option<ImageData>;

/// Render markdown to a flex-column Element with no image resolution.
/// Images are rendered as URL-text placeholders.
pub fn render_markdown(md: &str, opts: &MarkdownOptions) -> Element {
    render_markdown_with(md, opts, &|_| None)
}

/// Render markdown with an image resolver. For each `![alt](url)` the
/// resolver is called; if it returns `Some(data)` the image is placed into
/// the tree as an `image()` element, otherwise a text placeholder is used.
pub fn render_markdown_with(md: &str, opts: &MarkdownOptions, resolver: ImageResolver) -> Element {
    render_markdown_full(md, opts, resolver).0
}

/// Render markdown and also return a table of contents (images resolved
/// through `resolver`).
pub fn render_markdown_full(
    md: &str,
    opts: &MarkdownOptions,
    resolver: ImageResolver,
) -> (Element, Vec<TocEntry>) {
    let mut features = Options::empty();
    features.insert(Options::ENABLE_STRIKETHROUGH);
    features.insert(Options::ENABLE_TABLES);
    features.insert(Options::ENABLE_TASKLISTS);
    features.insert(Options::ENABLE_FOOTNOTES);
    let parser = Parser::new_ext(md, features);

    let mut builder = Builder::new(opts, resolver);
    for ev in parser {
        builder.event(ev);
    }
    builder.finish()
}

/// Render markdown and also return a TOC (no image resolver).
pub fn render_markdown_with_toc(md: &str, opts: &MarkdownOptions) -> (Element, Vec<TocEntry>) {
    render_markdown_full(md, opts, &|_| None)
}

// ---------------------------------------------------------------------------
// Builder — walks the event stream and accumulates Elements.
// ---------------------------------------------------------------------------

struct Builder<'a> {
    theme: MarkdownTheme,
    link_prefix: String,
    resolver: ImageResolver<'a>,
    toc: Vec<TocEntry>,
    blocks: Vec<Element>,

    // Inline accumulation state.
    inline: String,
    in_code: bool,
    in_strong: usize,
    in_em: usize,

    // Block state stacks.
    block_ctx: Vec<BlockCtx>,

    // List state.
    list_stack: Vec<ListState>,
    // Current heading (if inside one), None otherwise.
    heading: Option<HeadingLevel>,
    // Current link destination (if inside one).
    link_dest: Option<String>,
    // When inside an `![alt](url)`, inline events (the alt text) are dropped
    // so the alt doesn't leak into the enclosing paragraph. Many HTML→MD
    // conversions use the same alt on every `<img>` in an article (e.g.,
    // Famitsu uses the article summary); rendering that under every image
    // produces a duplicate caption that feels like the same text being
    // "reused forever". We prefer no caption over a wrong one.
    in_image: u32,
    saved_inline: Vec<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum BlockCtx {
    Paragraph,
    Heading,
    CodeBlock,
    BlockQuote,
    Item,
}

struct ListState {
    ordered: bool,
    next_number: u64,
}

impl<'a> Builder<'a> {
    fn new(opts: &MarkdownOptions, resolver: ImageResolver<'a>) -> Self {
        let link_prefix = if opts.link_id_prefix.is_empty() {
            "md-link:".to_string()
        } else {
            opts.link_id_prefix.clone()
        };
        Self {
            theme: opts.theme.clone(),
            link_prefix,
            resolver,
            toc: Vec::new(),
            blocks: Vec::new(),
            inline: String::new(),
            in_code: false,
            in_strong: 0,
            in_em: 0,
            block_ctx: Vec::new(),
            list_stack: Vec::new(),
            heading: None,
            link_dest: None,
            in_image: 0,
            saved_inline: Vec::new(),
        }
    }

    fn finish(self) -> (Element, Vec<TocEntry>) {
        let theme = self.theme;
        let root = div()
            .flex_col()
            .gap(theme.paragraph_gap)
            .children(self.blocks);
        (root, self.toc)
    }

    fn current_ctx(&self) -> Option<BlockCtx> {
        self.block_ctx.last().copied()
    }

    fn event(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag_end) => self.end(tag_end),
            Event::Text(s) => self.inline.push_str(&s),
            Event::Code(s) => {
                if self.in_code || matches!(self.current_ctx(), Some(BlockCtx::CodeBlock)) {
                    self.inline.push_str(&s);
                } else {
                    self.inline.push('`');
                    self.inline.push_str(&s);
                    self.inline.push('`');
                }
            }
            Event::SoftBreak => self.inline.push(' '),
            Event::HardBreak => self.inline.push('\n'),
            Event::Rule => self.blocks.push(hr(&self.theme)),
            Event::TaskListMarker(checked) => {
                self.inline.push_str(if checked { "☑ " } else { "☐ " });
            }
            Event::Html(s) | Event::InlineHtml(s) => self.inline.push_str(&s),
            Event::FootnoteReference(_) => {}
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.block_ctx.push(BlockCtx::Paragraph),
            Tag::Heading { level, .. } => {
                self.block_ctx.push(BlockCtx::Heading);
                self.heading = Some(level);
            }
            Tag::CodeBlock(_) => {
                self.block_ctx.push(BlockCtx::CodeBlock);
                self.in_code = true;
            }
            Tag::BlockQuote(_) => self.block_ctx.push(BlockCtx::BlockQuote),
            Tag::List(start) => self.list_stack.push(ListState {
                ordered: start.is_some(),
                next_number: start.unwrap_or(1),
            }),
            Tag::Item => self.block_ctx.push(BlockCtx::Item),
            Tag::Emphasis => self.in_em += 1,
            Tag::Strong => self.in_strong += 1,
            Tag::Link { dest_url, .. } => {
                self.link_dest = Some(dest_url.to_string());
            }
            Tag::Image { dest_url, title, .. } => {
                let block = image_block(&self.theme, &dest_url, &title, self.resolver);
                self.blocks.push(block);
                // Save the inline-in-progress and swallow any alt-text events
                // until TagEnd::Image. See `in_image` doc comment for why.
                self.saved_inline.push(std::mem::take(&mut self.inline));
                self.in_image += 1;
            }
            Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => {
                // tables: flatten to inline for now
            }
            _ => {}
        }
    }

    fn end(&mut self, tag_end: TagEnd) {
        match tag_end {
            TagEnd::Paragraph => {
                self.flush_inline_as(BlockCtx::Paragraph);
                self.block_ctx.pop();
            }
            TagEnd::Heading(_) => {
                let text_s = strip_orphan_markers(&std::mem::take(&mut self.inline));
                if let Some(level) = self.heading.take() {
                    let idx = heading_index(level);
                    let slug = slugify(&text_s);
                    self.toc.push(TocEntry {
                        id: slug.clone(),
                        text: text_s.clone(),
                        depth: idx as u32 + 1,
                    });
                    self.blocks.push(heading_block(&self.theme, idx, &text_s, &slug));
                }
                self.block_ctx.pop();
            }
            TagEnd::CodeBlock => {
                let code = std::mem::take(&mut self.inline);
                self.blocks.push(code_block(&self.theme, code.trim_end_matches('\n')));
                self.in_code = false;
                self.block_ctx.pop();
            }
            TagEnd::BlockQuote(_) => {
                self.flush_inline_as(BlockCtx::BlockQuote);
                self.block_ctx.pop();
            }
            TagEnd::List(_) => {
                self.list_stack.pop();
            }
            TagEnd::Item => {
                let marker = self.item_marker();
                let content = strip_orphan_markers(&std::mem::take(&mut self.inline));
                if !content.trim().is_empty() {
                    self.blocks.push(list_item(&self.theme, &marker, content.trim()));
                }
                self.block_ctx.pop();
            }
            TagEnd::Emphasis => {
                if self.in_em > 0 {
                    self.in_em -= 1;
                }
            }
            TagEnd::Strong => {
                if self.in_strong > 0 {
                    self.in_strong -= 1;
                }
            }
            TagEnd::Image => {
                // Discard any alt-text events accumulated while in_image and
                // restore the surrounding inline buffer.
                self.inline = self.saved_inline.pop().unwrap_or_default();
                if self.in_image > 0 {
                    self.in_image -= 1;
                }
            }
            TagEnd::Link => {
                let url = self.link_dest.take().unwrap_or_default();
                let label = std::mem::take(&mut self.inline);
                // Append a marker so the surrounding block shows the link URL;
                // a real inline link widget lands when Sabitori supports rich
                // inline formatting. Until then this keeps the URL visible and
                // we emit an id-tagged span as part of the paragraph.
                let shown = if label.is_empty() { url.clone() } else { label };
                self.inline.push_str(&shown);
                // leave URL in plain text so user can still see it when the
                // content is copied out. The link id is emitted when the block
                // flushes; we don't have inline id support yet.
                if !url.is_empty() && url != shown {
                    self.inline.push_str(" (");
                    self.inline.push_str(&url);
                    self.inline.push(')');
                }
            }
            _ => {}
        }
    }

    fn item_marker(&mut self) -> String {
        if let Some(state) = self.list_stack.last_mut() {
            if state.ordered {
                let n = state.next_number;
                state.next_number += 1;
                format!("{n}. ")
            } else {
                "• ".to_string()
            }
        } else {
            "• ".to_string()
        }
    }

    fn flush_inline_as(&mut self, ctx: BlockCtx) {
        let content = strip_orphan_markers(&std::mem::take(&mut self.inline));
        let content = content.trim();
        if content.is_empty() {
            return;
        }
        match ctx {
            BlockCtx::Paragraph => self.blocks.push(paragraph_block(&self.theme, content)),
            BlockCtx::BlockQuote => self.blocks.push(quote_block(&self.theme, content)),
            _ => self.blocks.push(paragraph_block(&self.theme, content)),
        }
    }
}

// ---------------------------------------------------------------------------
// Block builders
// ---------------------------------------------------------------------------

/// Strip literal `**`, `*`, and `` ` `` sequences that survive from
/// source markdown pulldown-cmark couldn't pair — unbalanced emphasis in
/// malformed input, `rawBody` JSON leaks from readability, etc. We don't
/// render inline bold/italic yet, so leaving orphan markers visible is
/// just noise.
///
/// Rule: drop `**` unconditionally, and drop single `*` / `` ` `` only
/// when they sit at a whitespace / punctuation boundary (to avoid eating
/// asterisks that are part of text like `a*b` or `C*`). Only scans ASCII
/// bytes — multi-byte UTF-8 continuation bytes never equal `*` or `` ` ``
/// so they pass through untouched.
fn strip_orphan_markers(s: &str) -> String {
    let s = s.as_bytes();
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if i + 1 < s.len() && s[i] == b'*' && s[i + 1] == b'*' {
            i += 2;
            continue;
        }
        if s[i] == b'*' || s[i] == b'`' {
            let at_boundary = i == 0
                || s[i - 1].is_ascii_whitespace()
                || matches!(s[i - 1], b'(' | b'[' | b'\n');
            let next_boundary = i + 1 >= s.len()
                || s[i + 1].is_ascii_whitespace()
                || matches!(
                    s[i + 1],
                    b')' | b']' | b'\n' | b':' | b'.' | b',' | b'!' | b'?'
                );
            if at_boundary || next_boundary {
                i += 1;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    String::from_utf8(out).expect("strip_orphan_markers preserves UTF-8")
}

fn heading_index(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 0,
        HeadingLevel::H2 => 1,
        HeadingLevel::H3 => 2,
        HeadingLevel::H4 => 3,
        HeadingLevel::H5 => 4,
        HeadingLevel::H6 => 5,
    }
}

fn heading_block(theme: &MarkdownTheme, idx: usize, text_s: &str, slug: &str) -> Element {
    let size = theme.heading_sizes[idx.min(5)];
    let el = text(text_s).font_size(size).color(theme.heading).bold();
    div().id(format!("heading:{slug}")).child(el)
}

fn paragraph_block(theme: &MarkdownTheme, s: &str) -> Element {
    text(s).font_size(theme.base_font_size).color(theme.body)
}

fn quote_block(theme: &MarkdownTheme, s: &str) -> Element {
    div()
        .flex_row()
        .gap(10.0)
        .children([
            div().w(Px(3.0)).bg(theme.quote_bar).rounded_px(2.0),
            text(s)
                .font_size(theme.base_font_size)
                .color(theme.dim),
        ])
}

fn code_block(theme: &MarkdownTheme, code: &str) -> Element {
    div()
        .bg(theme.code_bg)
        .rounded_px(6.0)
        .p(Px(12.0))
        .child(
            text(code)
                .font_size(theme.code_font_size)
                .color(theme.code_fg)
                .mono(),
        )
}

fn list_item(theme: &MarkdownTheme, marker: &str, content: &str) -> Element {
    div().flex_row().gap(6.0).children([
        text(marker)
            .font_size(theme.base_font_size)
            .color(theme.link),
        text(content).font_size(theme.base_font_size).color(theme.body),
    ])
}

fn hr(theme: &MarkdownTheme) -> Element {
    div().w(Dimension::Percent(1.0)).h(Px(1.0)).bg(theme.rule)
}

fn image_block(theme: &MarkdownTheme, url: &str, title: &str, resolver: ImageResolver) -> Element {
    // 記事本文中の画像は読みやすさ優先で「枠 (max_w × max_h) に収まる最大スケール、
    // ただし拡大はしない」 で表示する。 portrait 画像は枠より狭い幅で縦に表示、
    // landscape はフル幅、 小さい画像は等倍 (pixelate 防止)。
    // max_h は max_w * 1.2 で、 記事 1 ページ内で 1 枚が画面を占有しすぎない目安。
    let max_w = theme.max_image_width;
    let max_h = (max_w * 1.2).round();
    let default_w = max_w;
    let default_h = (max_w * 0.6).round();

    if let Some(data) = resolver(url) {
        let nat_w = data.width.max(1) as f32;
        let nat_h = data.height.max(1) as f32;
        let scale = (max_w / nat_w).min(max_h / nat_h).min(1.0);
        let block_w = (nat_w * scale).round();
        let block_h = (nat_h * scale).round();
        return div()
            .id(format!("md-image:{url}"))
            .w(Px(block_w))
            .h(Px(block_h))
            .child(
                img_element(url, data)
                    .w(Px(block_w))
                    .h(Px(block_h))
                    .rounded_px(6.0)
                    .object_fit(ObjectFit::Contain),
            );
    }
    let block_w = default_w;
    let block_h = default_h;

    let mut label = String::from("[image");
    if !title.is_empty() {
        label.push_str(": ");
        label.push_str(title);
    }
    label.push_str("]");
    // URL を全文出すと data:image/png;base64,... が数 KB 流れて UI が崩壊する。
    // - data: URL は scheme + media type だけ (例: `data:image/png`)
    // - 通常の HTTP URL は 80 文字でカット
    let short = if let Some(rest) = url.strip_prefix("data:") {
        let head = rest.split([',', ';']).next().unwrap_or("");
        format!("data:{head}")
    } else if url.chars().count() > 80 {
        let truncated: String = url.chars().take(77).collect();
        format!("{truncated}…")
    } else {
        url.to_string()
    };
    if !short.is_empty() {
        label.push(' ');
        label.push_str(&short);
    }
    div()
        .id(format!("md-image:{url}"))
        .w(Px(block_w))
        .h(Px(block_h))
        .bg(theme.code_bg)
        .rounded_px(6.0)
        .p(Px(10.0))
        .child(
            text(&label)
                .font_size(theme.code_font_size)
                .color(theme.dim)
                .mono(),
        )
}

// ---------------------------------------------------------------------------
// Slug generation — mirrors rehype-slug: lowercase, spaces→dashes, strip
// anything that isn't [a-z0-9-].
// ---------------------------------------------------------------------------

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if c.is_whitespace() || c == '-' || c == '_' {
            if !prev_dash && !out.is_empty() {
                out.push('-');
                prev_dash = true;
            }
        } else if !c.is_ascii() {
            // Keep non-ASCII characters verbatim (Japanese / CJK headings etc.).
            out.push(ch);
            prev_dash = false;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        let (el, toc) = render_markdown_with_toc(
            "# Title\n\nA paragraph with **bold** text.\n\n- one\n- two\n\n```\nfn main() {}\n```\n",
            &MarkdownOptions { extract_toc: true, ..Default::default() },
        );
        // at least heading + paragraph + two list items + code block
        assert!(el.children.len() >= 5, "expected ≥ 5 blocks, got {}", el.children.len());
        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].text, "Title");
    }

    #[test]
    fn slug_keeps_japanese() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("見出し テスト"), "見出し-テスト");
    }
}
