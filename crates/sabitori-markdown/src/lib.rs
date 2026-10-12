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
//! blockquotes, tables) are preserved.
//!
//! # Example
//! ```no_run
//! use sabitori_markdown::{render_markdown, MarkdownOptions};
//! let opts = MarkdownOptions::default();
//! let element = render_markdown("# Hello\n\nWorld", &opts);
//! ```

use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use sabitori_core::element::{
    div, image as img_element, text, AlignItems, AlignSelf, Dimension, Element, GridPlacement,
    ImageData, ObjectFit, Px, TextAlign, Track,
};
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
    // 組みかけの表 (表の中なら Some)。
    table: Option<TableState>,
}

/// 組みかけの表。セルの文字は段落と同じく平たい文字にして溜める。
struct TableState {
    aligns: Vec<Alignment>,
    head: Vec<String>,
    rows: Vec<Vec<String>>,
    // 組みかけの行 (見出しの行も)。
    row: Vec<String>,
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
            table: None,
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
            Tag::Table(aligns) => {
                // 表の前に溜まっていた文字はセルに混ぜない (表のあとで戻す)。
                self.saved_inline.push(std::mem::take(&mut self.inline));
                self.table = Some(TableState {
                    aligns,
                    head: Vec::new(),
                    rows: Vec::new(),
                    row: Vec::new(),
                });
            }
            Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
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
            TagEnd::TableCell => {
                let cell = strip_orphan_markers(&std::mem::take(&mut self.inline));
                if let Some(table) = self.table.as_mut() {
                    table.row.push(cell.trim().to_string());
                }
            }
            TagEnd::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.head = std::mem::take(&mut table.row);
                }
            }
            TagEnd::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    let row = std::mem::take(&mut table.row);
                    table.rows.push(row);
                }
            }
            TagEnd::Table => {
                if let Some(table) = self.table.take() {
                    self.blocks.push(table_block(&self.theme, &table));
                }
                self.inline = self.saved_inline.pop().unwrap_or_default();
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
    // 縦並びにする — 横並び (`div()` の既定) だと長い見出しが 1 行の「…」に切れる。
    div().id(format!("heading:{slug}")).flex_col().child(el)
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
            // 横並びの中の文は、最小幅 0 で折り返させる (書かないと 1 行の「…」)。
            text(s)
                .font_size(theme.base_font_size)
                .color(theme.dim)
                .grow(1.0)
                .min_w(Px(0.0)),
        ])
}

fn code_block(theme: &MarkdownTheme, code: &str) -> Element {
    div()
        .flex_col()
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

/// GFM の表。列は中身を 1 行に並べた幅まで広がり、入りきらなければ親の幅を
/// 分け合ってセルの中で折り返す。表そのものも中身の幅で、横いっぱいには伸ばさない。
fn table_block(theme: &MarkdownTheme, table: &TableState) -> Element {
    let cols = table
        .aligns
        .len()
        .max(table.head.len())
        .max(table.rows.iter().map(Vec::len).max().unwrap_or(0));
    if cols == 0 {
        return div();
    }
    let cell = |s: &str, col: usize, head: bool| -> Element {
        // 揃えは列ごと (区切り行の `:-:` / `--:`)。折り返した 2 行目からも揃える。
        let (items, align) = match table.aligns.get(col) {
            Some(Alignment::Center) => (AlignItems::Center, TextAlign::Center),
            Some(Alignment::Right) => (AlignItems::End, TextAlign::End),
            _ => (AlignItems::Stretch, TextAlign::Start),
        };
        let label = text(s)
            .font_size(theme.base_font_size)
            .color(if head { theme.heading } else { theme.body })
            .text_align(align);
        let cell = div()
            .flex_col()
            .align_items(items)
            .px_pad(Px(10.0))
            .py(Px(6.0))
            .child(if head { label.bold() } else { label });
        if head { cell.bg(theme.code_bg) } else { cell }
    };
    // 行の間の罫線。全部の列をまたぐ 1 本。
    let rule = || {
        div()
            .h(Px(1.0))
            .bg(theme.rule)
            .col(GridPlacement::Line(1), GridPlacement::Line(-1))
    };
    // 足りないセル (見出しより短い行) は空で埋める
    fn at(row: &[String], col: usize) -> &str {
        row.get(col).map_or("", String::as_str)
    }
    let mut cells = Vec::with_capacity((table.rows.len() + 1) * (cols + 1));
    cells.extend((0..cols).map(|c| cell(at(&table.head, c), c, true)));
    for row in &table.rows {
        cells.push(rule());
        cells.extend((0..cols).map(|c| cell(at(row, c), c, false)));
    }
    div()
        // 下限 0・上限は中身の幅。入るなら中身の幅、入らなければ余りを分け合う
        .grid_cols(Track::repeat(cols, Track::minmax(Track::px(0.0), Track::max_content())))
        .align_self(AlignSelf::Start)
        .border(1.0, theme.rule)
        .children(cells)
}

fn list_item(theme: &MarkdownTheme, marker: &str, content: &str) -> Element {
    div().flex_row().gap(6.0).children([
        text(marker)
            .font_size(theme.base_font_size)
            .color(theme.link),
        text(content).font_size(theme.base_font_size).color(theme.body).grow(1.0).min_w(Px(0.0)),
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

    use sabitori_core::build::{
        approx_caret, build_tree_measured, BuildResult, CaretPos, TextMeasure, TextShape,
    };
    use sabitori_core::element::ElementKind;
    use sabitori_core::render_list::{RenderCommand, TextDraw};
    use sabitori_core::{Rect, Size, TextMetrics, Typography};

    /// 木の中の文字を、書いた順に。
    fn texts(el: &Element) -> Vec<String> {
        fn walk(el: &Element, out: &mut Vec<String>) {
            if let ElementKind::Text { content } = &el.kind {
                out.push(content.to_string());
            }
            for child in &el.children {
                walk(child, out);
            }
        }
        let mut out = Vec::new();
        walk(el, &mut out);
        out
    }

    #[test]
    fn a_table_keeps_its_cells_apart_and_out_of_the_next_paragraph() {
        // 前は表を平たい文字のまま流していて、セルが区切り無しで次の段落の頭に付いた
        let el = render_markdown(
            "| a | b |\n|---|---|\n| 1 | 2 |\n\nafter\n",
            &MarkdownOptions::default(),
        );
        assert_eq!(el.children.len(), 2, "表と段落の 2 つ");
        assert_eq!(texts(&el.children[0]), ["a", "b", "1", "2"]);
        assert_eq!(texts(&el.children[1]), ["after"]);
    }

    #[test]
    fn a_table_at_the_end_of_the_document_is_not_lost() {
        // 前は流れ込む次の段落が無いので、文書の最後の表は丸ごと消えていた
        let el = render_markdown("| a |\n|---|\n| 1 |\n", &MarkdownOptions::default());
        assert_eq!(texts(&el), ["a", "1"]);
    }

    #[test]
    fn a_short_row_gets_empty_cells() {
        let el = render_markdown("| a | b |\n|---|---|\n| 1 |\n", &MarkdownOptions::default());
        assert_eq!(texts(&el), ["a", "b", "1", ""]);
    }

    #[test]
    fn text_before_a_table_stays_out_of_its_cells() {
        let el = render_markdown(
            "- item\n\n| a |\n|---|\n| 1 |\n",
            &MarkdownOptions::default(),
        );
        let table = el.children.last().expect("表がある");
        assert_eq!(texts(table), ["a", "1"]);
    }

    /// 1 文字 = 字の大きさの半分の幅。渡された幅で折り返し、行の数だけ高くなる。
    struct Wrapping;

    impl TextMeasure for Wrapping {
        fn measure(
            &self,
            content: &str,
            font_size: f32,
            _bold: bool,
            _monospace: bool,
            _font_family: Option<&str>,
            max_width: Option<f32>,
            _max_lines: Option<u32>,
            _typo: Typography,
        ) -> TextMetrics {
            let natural = content.chars().count() as f32 * font_size * 0.5;
            let width = match max_width {
                Some(w) if w > 0.0 && w < natural => w,
                _ => natural,
            };
            let lines = (natural / width.max(1.0)).ceil().max(1.0);
            TextMetrics {
                size: Size { width, height: lines * font_size },
                baseline: font_size * 0.8,
            }
        }

        fn caret_pos(&self, content: &str, byte_offset: usize, shape: TextShape<'_>) -> CaretPos {
            approx_caret::caret_pos(content, byte_offset, shape.font_size * 0.5, shape.font_size)
        }

        fn offset_at(&self, content: &str, point: (f32, f32), shape: TextShape<'_>) -> usize {
            approx_caret::offset_at(content, point, shape.font_size * 0.5, shape.font_size)
        }

        fn range_rects(&self, content: &str, range: (usize, usize), shape: TextShape<'_>) -> Vec<Rect> {
            approx_caret::range_rects(content, range, shape.font_size * 0.5, shape.font_size)
        }
    }

    fn drawn<'a>(build: &'a BuildResult, s: &str) -> &'a TextDraw {
        build
            .render_list
            .commands
            .iter()
            .find_map(|c| match c {
                RenderCommand::Text(t) if &*t.content == s => Some(t),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{s:?} が描かれていない"))
    }

    /// 表の外枠 (枠線だけの矩形) の幅。
    fn frame_width(build: &BuildResult) -> f32 {
        build
            .render_list
            .commands
            .iter()
            .find_map(|c| match c {
                RenderCommand::Rect(r) if r.border_width == 1.0 => Some(r.rect.size.width),
                _ => None,
            })
            .expect("表の枠がある")
    }

    #[test]
    fn columns_line_up_and_the_table_is_only_as_wide_as_its_content() {
        let el = render_markdown(
            "| name | size |\n|---|--:|\n| a.txt | 12 |\n| longer-name.md | 3456 |\n",
            &MarkdownOptions::default(),
        );
        let build = build_tree_measured(&el, 600.0, 400.0, &Wrapping);
        let (name, a, long) = (drawn(&build, "name"), drawn(&build, "a.txt"), drawn(&build, "longer-name.md"));
        assert_eq!(name.position.x, a.position.x, "同じ列は同じ左端");
        assert_eq!(a.position.x, long.position.x, "同じ列は同じ左端");
        let size = drawn(&build, "size");
        assert!(
            size.position.x >= long.position.x + long.max_width,
            "2 列目は 1 列目の一番長いセルより右: {} < {}",
            size.position.x,
            long.position.x + long.max_width
        );
        // `--:` の列は右で揃う
        let (n12, n3456) = (drawn(&build, "12"), drawn(&build, "3456"));
        assert_eq!(n12.position.x + n12.max_width, n3456.position.x + n3456.max_width);
        assert!(frame_width(&build) < 300.0, "横いっぱいに伸ばさない: {}", frame_width(&build));
    }

    #[test]
    fn a_table_wider_than_its_box_wraps_inside_the_cells() {
        let long = "word ".repeat(40);
        let long = long.trim();
        let el = render_markdown(
            &format!("| key | value |\n|---|---|\n| k | {long} |\n"),
            &MarkdownOptions::default(),
        );
        let build = build_tree_measured(&el, 300.0, 2000.0, &Wrapping);
        let (key, value) = (drawn(&build, "key"), drawn(&build, long));
        assert!(
            value.position.x >= key.position.x + key.max_width,
            "列は重ならない: {} < {}",
            value.position.x,
            key.position.x + key.max_width
        );
        assert!(
            value.position.x + value.max_width <= 300.0,
            "箱からはみ出さない: {}",
            value.position.x + value.max_width
        );
        assert!(value.max_height > 15.0 * 2.0, "折り返して何行にもなる: {}", value.max_height);
        assert!(frame_width(&build) <= 300.0, "表も箱の幅まで: {}", frame_width(&build));
    }
}
