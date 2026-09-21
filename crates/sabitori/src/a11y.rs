//! **支援技術へツリーを渡す** ([#25](https://github.com/Mutafika/sabitori/issues/25))。
//!
//! [#21] で意味層 (`Role` / `label` / `heading`) は入ったが、OS へ渡す部分が
//! 無かったので **VoiceOver / NVDA からは空の窓に見えていた**。実際、macOS の
//! アクセシビリティ API で覗くと `AXApplication` の下に窓が 1 枚も無く、
//! 外から窓を探す自動化も掴めない (#75 の 17 で「窓を取り違える」と報告された
//! のもこれが背景)。
//!
//! # やっていること
//!
//! 1. 毎フレームの [`BuildResult`] を [`accesskit::TreeUpdate`] に変換する
//! 2. `accesskit_winit::Adapter` に渡す (**支援技術が起きているときだけ**
//!    変換が走る — `update_if_active` が閉じていれば中身を呼ばない)
//! 3. 支援技術からの操作 (クリック・フォーカス) を受けてアプリへ流す
//!
//! # ツリーの作り方
//!
//! `hit_regions` には id / 矩形 / 役割 / ラベル / フォーカス可否が揃っている。
//! 構造 (入れ子) は持っていないので、**根の下に平らに並べる**。読み上げの順は
//! `element_index` (深さ優先の添字) 順 = 書いた順。
//!
//! 名前 (読み上げられる文字列) は `.label()` があればそれ、無ければ
//! **その矩形の中にある文字**を拾う。ボタンの中の `text()` がそのまま
//! ボタンの名前になるので、アプリは `.label()` を書かなくても名乗れる。
//! どの領域にも入らない文字は、それ自体を読み上げ対象の節として並べる。
//!
//! # 分かっていること (v0.13.0 時点)
//!
//! - **見えているものだけ**が並ぶ。`hit_regions` も描画命令も、スクロールで
//!   画面外に出たぶんは丸ごと落ちるので、支援技術からも見えない。長い一覧は
//!   スクロールしてはじめて読める (画面を描き直すたびに送り直すので、
//!   スクロールすれば追随はする)。
//! - ツリーは**平ら**。入れ子の構造 (表 → 行 → セル) は役割としては渡るが、
//!   親子関係にはなっていない。
//! - 窓は 1 枚ぶん。`extra_windows` で開いた窓には付けていない
//!   (あちらは入力を取らない描画専用)。
//! - wasm では何もしない。ブラウザ側は DOM を出す話になるので別の設計が要る。

use accesskit::{Action, Node, NodeId, Rect as AxRect, Role as AxRole, TreeUpdate};
use sabitori_core::build::{BuildResult, HitRegion};
use sabitori_core::element::Role;
use sabitori_core::{RenderCommand, Rect};

/// 根 (窓) の節。
pub(crate) const ROOT: NodeId = NodeId(0);

/// 文字だけの節に振る id の始まり。領域の id (= `element_index + 1`) と
/// ぶつからないよう、届かないところから始める。
const TEXT_ID_BASE: u64 = 1 << 32;

/// 変換の結果。`TreeUpdate` と、節 → 要素 id の対応表。
///
/// 対応表が無いと、支援技術から「この節を押した」と言われたときに、
/// どの要素のことか分からない。
pub(crate) struct A11yTree {
    pub(crate) update: TreeUpdate,
    pub(crate) ids: std::collections::HashMap<u64, String>,
}

/// sabitori の役割を accesskit の役割へ。
fn ax_role(role: Role) -> AxRole {
    match role {
        Role::Group => AxRole::Group,
        Role::Button => AxRole::Button,
        Role::Link => AxRole::Link,
        Role::TextInput => AxRole::TextInput,
        Role::TextArea => AxRole::MultilineTextInput,
        Role::Password => AxRole::PasswordInput,
        Role::Checkbox => AxRole::CheckBox,
        Role::Radio => AxRole::RadioButton,
        Role::Slider => AxRole::Slider,
        Role::ComboBox => AxRole::ComboBox,
        Role::Heading => AxRole::Heading,
        Role::Text => AxRole::Label,
        Role::Image => AxRole::Image,
        Role::List => AxRole::List,
        Role::ListItem => AxRole::ListItem,
        Role::TabList => AxRole::TabList,
        Role::Tab => AxRole::Tab,
        Role::Dialog => AxRole::Dialog,
        Role::ProgressBar => AxRole::ProgressIndicator,
        Role::Separator => AxRole::Splitter,
        Role::Table => AxRole::Table,
        Role::Row => AxRole::Row,
        Role::Cell => AxRole::Cell,
        Role::ColumnHeader => AxRole::ColumnHeader,
        Role::Tree => AxRole::Tree,
        Role::TreeItem => AxRole::TreeItem,
    }
}

fn ax_rect(r: Rect) -> AxRect {
    AxRect {
        x0: r.origin.x as f64,
        y0: r.origin.y as f64,
        x1: (r.origin.x + r.size.width) as f64,
        y1: (r.origin.y + r.size.height) as f64,
    }
}

/// この領域を支援技術に見せるか。
///
/// **押せる / 焦点が当たる / 役割かラベルを名乗った / 中に文字がある** の
/// どれかだけ。ただのレイアウトの入れ物まで並べると、読み上げが
/// 「グループ、グループ、グループ」で埋まる。
fn is_meaningful(region: &HitRegion, has_text: bool) -> bool {
    region.role.is_some()
        || region.label.is_some()
        || region.focusable
        || region.has_click_handler
        || has_text
}

/// 文字が入っている領域のうち**いちばん内側**を選ぶ。
///
/// 外側から順に見ると、ボタンの文字が画面全体の入れ物の名前になってしまう。
/// 面積がいちばん小さいものを選べば、実際にその文字を持っている要素になる。
fn innermost_containing(regions: &[&HitRegion], point: sabitori_core::Point) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, r) in regions.iter().enumerate() {
        if !r.rect.contains(point) {
            continue;
        }
        let area = r.rect.size.width * r.rect.size.height;
        if best.is_none_or(|(_, b)| area < b) {
            best = Some((i, area));
        }
    }
    best.map(|(i, _)| i)
}

/// 1 フレームのビルド結果を支援技術向けのツリーに変換する。
///
/// `scale` は論理 px → 物理 px の倍率。**accesskit の座標は物理 px** なので、
/// 根に倍率を掛けておいて中身は論理 px のまま渡す (掛け忘れると、Retina で
/// 読み上げ枠が実際の半分の位置に出る)。
pub(crate) fn tree_update(
    build: &BuildResult,
    title: &str,
    focused: Option<&str>,
    scale: f32,
) -> A11yTree {
    // 描いた順 (= 書いた順) に並べ直す。`hit_regions` は当たり判定のために
    // 手前から並んでいるので、そのまま読むと画面の読み上げが逆順になる。
    let mut regions: Vec<&HitRegion> = build.hit_regions.iter().collect();
    regions.sort_by_key(|r| r.element_index);

    // 文字を、それを持っている領域へ配る。
    let mut names: Vec<Vec<String>> = vec![Vec::new(); regions.len()];
    let mut loose: Vec<(usize, Rect, String)> = Vec::new();
    for cmd in &build.render_list.commands {
        let RenderCommand::Text(t) = cmd else { continue };
        if t.content.trim().is_empty() {
            continue;
        }
        // 文字の左上から少しだけ内側の点で判定する (境界のちょうど上だと、
        // 隣り合った領域のどちらに入るかが丸め次第になる)。
        let probe = sabitori_core::Point::new(t.position.x + 1.0, t.position.y + 1.0);
        match innermost_containing(&regions, probe) {
            Some(i) => names[i].push(t.content.to_string()),
            None => loose.push((
                t.element_index,
                Rect::new(t.position.x, t.position.y, t.max_width, t.font_size * 1.4),
                t.content.to_string(),
            )),
        }
    }

    let mut nodes: Vec<(NodeId, Node)> = Vec::new();
    // 読み上げの順は**書いた順**。当たり領域も、どこにも属さない文字も、
    // 同じ深さ優先の添字で並べ直す — 領域を先に全部並べてから文字を足すと、
    // 「ボタン、テキスト欄、保存、保存した回数: 0」のように本文が最後へ回る。
    let mut ordered: Vec<(usize, NodeId)> = Vec::new();
    let mut ids = std::collections::HashMap::new();
    let mut seen: std::collections::HashSet<NodeId> = std::collections::HashSet::new();
    let mut focus = ROOT;

    for (i, region) in regions.iter().enumerate() {
        let text = names[i].join(" ");
        if !is_meaningful(region, !text.is_empty()) {
            continue;
        }
        let node_id = NodeId(region.element_index as u64 + 1);
        let role = region.role.map(ax_role).unwrap_or(AxRole::Group);
        let mut node = Node::new(role);
        node.set_bounds(ax_rect(region.rect));

        let label = region.label.clone().unwrap_or(text);
        if !label.is_empty() {
            node.set_label(label);
        }
        if let Some(level) = region.heading_level {
            // accesskit の level は 0 起点 (ARIA は 1 起点)。
            node.set_level((level.max(1) as usize) - 1);
        }
        if region.disabled {
            node.set_disabled();
        } else {
            // 「押せる」の基準はランタイムの押下解決と揃える (`clickable`)。
            // `has_click_handler` にすると、宣言的な `.click(ctx, ..)` で
            // 配線したボタンが**押せないものとして読み上げられる** —
            // あちらは `Element::on_click` を立てず、id で処理表を引くため。
            if region.clickable {
                node.add_action(Action::Click);
            }
            if region.focusable {
                node.add_action(Action::Focus);
            }
        }
        if let Some(id) = region.id.as_deref() {
            ids.insert(node_id.0, id.to_string());
            if focused == Some(id) {
                focus = node_id;
            }
        }
        // ★**同じ番号を 2 度渡さない。**accesskit は同じ子を 2 つ持つ
        // `TreeUpdate` を panic で断る (`TreeUpdate includes duplicate child`)
        // ので、ここで 1 つ落とすのと窓ごと落ちるのとの二択になる。番号が
        // ぶつからないようにするのは渡す側の仕事 (overlay は
        // `declarative::OVERLAY_INDEX_BASE` で別の帯に置いてある) で、
        // これはその取りこぼしが**窓を殺さない**ための受け皿。
        if !seen.insert(node_id) {
            continue;
        }
        ordered.push((region.element_index, node_id));
        nodes.push((node_id, node));
    }

    // どの領域にも入らない文字 (本文・見出し・ラベル)。これを落とすと、
    // 押せるものだけが読み上げられて**中身が無い画面**になる。
    for (i, (element_index, rect, content)) in loose.into_iter().enumerate() {
        let node_id = NodeId(TEXT_ID_BASE + i as u64);
        let mut node = Node::new(AxRole::Label);
        node.set_bounds(ax_rect(rect));
        node.set_label(content);
        ordered.push((element_index, node_id));
        nodes.push((node_id, node));
    }

    ordered.sort_by_key(|(index, _)| *index);
    let children: Vec<NodeId> = ordered.into_iter().map(|(_, id)| id).collect();

    let mut root = Node::new(AxRole::Window);
    root.set_label(title.to_string());
    root.set_children(children);
    // accesskit は物理 px。論理 px のまま渡せるよう、根で倍率を掛ける。
    root.set_transform(accesskit::Affine::scale(scale as f64));
    nodes.push((ROOT, root));

    A11yTree {
        update: TreeUpdate {
            nodes,
            tree: Some(accesskit::TreeInfo::new(ROOT)),
            tree_id: accesskit::TreeId::ROOT,
            focus,
        },
        ids,
    }
}

// ---------------------------------------------------------------------------
// ランタイムとの橋渡し
// ---------------------------------------------------------------------------

/// 支援技術から来た操作。要素 id に直してある。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Request {
    /// 押された (VoiceOver の Ctrl+Option+Space など)。
    Click(String),
    /// 焦点を移された。
    Focus(String),
}

/// ハンドラとランタイムのあいだで持つもの。
///
/// **ハンドラはどのスレッドから呼ばれるか分からない** (プラットフォーム次第)
/// ので、受けたものはここに積んで、次のフレームでランタイムが引き取る。
struct Shared {
    requests: std::sync::Mutex<Vec<accesskit::ActionRequest>>,
    window: std::sync::Arc<winit::window::Window>,
}

impl Shared {
    /// 1 フレーム起こす。**これが無いと届かない。** 既定の `lazy_render` は
    /// 入力が無ければ描かないので、支援技術からの操作は積まれたまま
    /// 誰も引き取らない (web の橋で踏んだのと同じ穴 — #73)。
    fn wake(&self) {
        self.window.request_redraw();
    }
}

struct Activation(std::sync::Arc<Shared>);

impl accesskit::ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        // ここでツリーを組めない (アプリの状態はこのスレッドから触れない)。
        // 起こしておけば、次のフレームの `update` が本物を送る。
        self.0.wake();
        None
    }
}

struct Actions(std::sync::Arc<Shared>);

impl accesskit::ActionHandler for Actions {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        if let Ok(mut q) = self.0.requests.lock() {
            q.push(request);
        }
        self.0.wake();
    }
}

struct Deactivation;

impl accesskit::DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {}
}

/// 窓 1 枚ぶんの支援技術アダプタ。
pub(crate) struct Bridge {
    adapter: accesskit_winit::Adapter,
    shared: std::sync::Arc<Shared>,
    /// 直近に送ったツリーの「節 → 要素 id」。操作が来たときに引く。
    ids: std::collections::HashMap<u64, String>,
}

impl Bridge {
    /// **窓を表示する前に**作ること (accesskit の要件。表示済みだと panic する)。
    pub(crate) fn new(
        event_loop: &winit::event_loop::ActiveEventLoop,
        window: &std::sync::Arc<winit::window::Window>,
    ) -> Self {
        let shared = std::sync::Arc::new(Shared {
            requests: std::sync::Mutex::new(Vec::new()),
            window: window.clone(),
        });
        let adapter = accesskit_winit::Adapter::with_direct_handlers(
            event_loop,
            window,
            Activation(shared.clone()),
            Actions(shared.clone()),
            Deactivation,
        );
        Self { adapter, shared, ids: std::collections::HashMap::new() }
    }

    /// winit のイベントをアダプタにも見せる (窓の移動・リサイズ・焦点)。
    pub(crate) fn process_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) {
        self.adapter.process_event(window, event);
    }

    /// 今フレームのツリーを送る。
    ///
    /// **支援技術が起きていなければ中身は走らない** (`update_if_active` が
    /// 閉じていれば変換ごと省かれる) ので、ふつうの起動で費用はほぼゼロ。
    pub(crate) fn update(
        &mut self,
        build: &BuildResult,
        title: &str,
        focused: Option<&str>,
        scale: f32,
    ) {
        let mut ids = None;
        self.adapter.update_if_active(|| {
            let tree = tree_update(build, title, focused, scale);
            ids = Some(tree.ids);
            tree.update
        });
        if let Some(ids) = ids {
            self.ids = ids;
        }
    }

    /// 溜まった操作を要素 id に直して取り出す。
    pub(crate) fn take_requests(&mut self) -> Vec<Request> {
        let raw = match self.shared.requests.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => return Vec::new(),
        };
        raw.into_iter()
            .filter_map(|r| {
                let id = self.ids.get(&r.target_node.0)?.clone();
                match r.action {
                    Action::Click => Some(Request::Click(id)),
                    Action::Focus => Some(Request::Focus(id)),
                    _ => None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_core::build::build_tree;
    use sabitori_core::element::{button, div, text, Px, Role};

    fn update_of(root: &sabitori_core::Element, focused: Option<&str>) -> A11yTree {
        let build = build_tree(root, 800.0, 600.0);
        tree_update(&build, "テスト", focused, 2.0)
    }

    fn node_of<'a>(t: &'a A11yTree, label: &str) -> Option<&'a Node> {
        t.update
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some(label))
            .map(|(_, n)| n)
    }

    /// **ボタンが役割と名前を名乗る。** 中の文字がそのまま名前になるので、
    /// アプリは `.label()` を書かなくてよい。
    #[test]
    fn a_button_announces_its_role_and_its_own_text() {
        let root = div()
            .w_full()
            .h_full()
            .child(button("保存").id("save").w(Px(80.0)).h(Px(32.0)));
        let t = update_of(&root, None);

        let n = node_of(&t, "保存").expect("ボタンが出ていない");
        assert_eq!(n.role(), AxRole::Button);
        assert!(n.supports_action(Action::Click), "押せると名乗っていない");
    }

    /// `.label()` を書いたらそちらが勝つ (アイコンだけのボタン)。
    #[test]
    fn an_explicit_label_wins_over_the_inner_text() {
        let root = div().w_full().h_full().child(
            div()
                .id("close")
                .role(Role::Button)
                .label("閉じる")
                .w(Px(24.0))
                .h(Px(24.0))
                .child(text("×")),
        );
        let t = update_of(&root, None);
        assert!(node_of(&t, "閉じる").is_some());
        assert!(node_of(&t, "×").is_none(), "見た目の文字が名前になっている");
    }

    /// **本文が読める。** 押せるものだけ並べると中身の無い画面になる。
    #[test]
    fn body_text_is_part_of_the_tree() {
        let root = div()
            .w_full()
            .h_full()
            .flex_col()
            .children([text("予約が 3 件あります"), text("本日分")]);
        let t = update_of(&root, None);
        assert!(node_of(&t, "予約が 3 件あります").is_some());
        assert!(node_of(&t, "本日分").is_some());
    }

    /// 読み上げの順が**書いた順**であること。手前から並べると逆に読まれる。
    #[test]
    fn nodes_are_listed_in_document_order() {
        let root = div().w_full().h_full().flex_col().children([
            button("1 番目").id("a").w(Px(80.0)).h(Px(30.0)),
            button("2 番目").id("b").w(Px(80.0)).h(Px(30.0)),
            button("3 番目").id("c").w(Px(80.0)).h(Px(30.0)),
        ]);
        let t = update_of(&root, None);
        let root_node = &t.update.nodes.iter().find(|(id, _)| *id == ROOT).unwrap().1;
        let order: Vec<String> = root_node
            .children()
            .iter()
            .filter_map(|id| {
                t.update
                    .nodes
                    .iter()
                    .find(|(nid, _)| nid == id)
                    .and_then(|(_, n)| n.label().map(|s| s.to_string()))
            })
            .collect();
        assert_eq!(order, vec!["1 番目", "2 番目", "3 番目"]);
    }

    /// 焦点のある要素が `focus` に載ること。載らないと、支援技術のカーソルが
    /// 画面の先頭に戻り続ける。
    #[test]
    fn the_focused_element_is_reported_as_focus() {
        let root = div().w_full().h_full().flex_col().children([
            div().id("name").focusable().role(Role::TextInput).w(Px(200.0)).h(Px(32.0)),
            div().id("memo").focusable().role(Role::TextInput).w(Px(200.0)).h(Px(32.0)),
        ]);
        let t = update_of(&root, Some("memo"));
        let focused_element_id = t.ids.get(&t.update.focus.0).map(|s| s.as_str());
        assert_eq!(focused_element_id, Some("memo"));

        // 焦点が無ければ根 (これも毎回渡す必要がある)。
        let t = update_of(&root, None);
        assert_eq!(t.update.focus, ROOT);
    }

    /// 無効な要素は「押せる」と名乗らない。
    #[test]
    fn a_disabled_button_does_not_offer_click() {
        let root = div()
            .w_full()
            .h_full()
            .child(button("保存").id("save").disabled(true).w(Px(80.0)).h(Px(32.0)));
        let t = update_of(&root, None);
        let n = node_of(&t, "保存").expect("ボタンが消えた");
        assert!(n.is_disabled(), "無効だと名乗っていない");
        assert!(!n.supports_action(Action::Click));
    }

    /// **ただの入れ物は並べない。** 並べると読み上げが「グループ」で埋まる。
    #[test]
    fn plain_layout_boxes_are_left_out() {
        let root = div().w_full().h_full().child(
            div().id("wrapper").w(Px(400.0)).h(Px(300.0)).child(
                div().id("inner").w(Px(200.0)).h(Px(100.0)),
            ),
        );
        let t = update_of(&root, None);
        // 根だけ。
        assert_eq!(t.update.nodes.len(), 1, "意味の無い入れ物が並んでいる");
    }

    /// 見出しの階層が渡ること (ARIA は 1 起点、accesskit は 0 起点)。
    #[test]
    fn a_heading_carries_its_level() {
        let root = div().w_full().h_full().child(text("車両一覧").heading(2).id("h"));
        let t = update_of(&root, None);
        let n = node_of(&t, "車両一覧").expect("見出しが無い");
        assert_eq!(n.role(), AxRole::Heading);
        assert_eq!(n.level(), Some(1), "2 階層目は 0 起点で 1");
    }

    /// **Retina で位置がずれない。** accesskit は物理 px なので、
    /// 根に倍率を持たせて中身は論理 px のまま渡す。
    #[test]
    fn the_root_carries_the_scale_factor() {
        let root = div().w_full().h_full().child(
            button("保存").id("save").w(Px(80.0)).h(Px(32.0)),
        );
        let t = update_of(&root, None);
        let root_node = &t.update.nodes.iter().find(|(id, _)| *id == ROOT).unwrap().1;
        let transform = root_node.transform().expect("倍率が乗っていない");
        assert_eq!(transform.as_coeffs()[0], 2.0);

        // 中身は論理 px のまま。
        let n = node_of(&t, "保存").unwrap();
        assert_eq!(n.bounds().unwrap().width(), 80.0);
    }
}

#[cfg(test)]
mod order_tests {
    //! 読み上げの順は**書いた順**。押せるものと本文が混ざっていても崩れないこと。

    use super::*;
    use sabitori_core::build::build_tree;
    use sabitori_core::element::{button, div, text, Px};

    fn labels_in_order(root: &sabitori_core::Element) -> Vec<String> {
        let build = build_tree(root, 800.0, 600.0);
        let t = tree_update(&build, "テスト", None, 1.0);
        let root_node = &t.update.nodes.iter().find(|(id, _)| *id == ROOT).unwrap().1;
        root_node
            .children()
            .iter()
            .filter_map(|id| {
                t.update
                    .nodes
                    .iter()
                    .find(|(nid, _)| nid == id)
                    .and_then(|(_, n)| n.label().map(|s| s.to_string()))
            })
            .collect()
    }

    /// **本文が最後に回らない。** 当たり領域を先に全部並べてから文字を足すと、
    /// 「見出し → 欄 → ボタン → 本文」と読まれて話の順が崩れる。
    #[test]
    fn body_text_keeps_its_place_between_controls() {
        let root = div().w_full().h_full().flex_col().children([
            text("車両一覧").id("h").heading(2),
            text("3 件あります"),
            button("追加").id("add").w(Px(80.0)).h(Px(30.0)),
            text("最終更新 10:30"),
        ]);
        assert_eq!(
            labels_in_order(&root),
            vec!["車両一覧", "3 件あります", "追加", "最終更新 10:30"]
        );
    }

    /// ★**同じ番号の領域を 2 つ渡されても窓を落とさない。**
    ///
    /// overlay は別のツリーとして組まれるので `element_index` が 0 から振り直され、
    /// 地の領域と混ざると同じ番号が 2 つ並ぶ。番号から `NodeId` を作っているので、
    /// そのまま渡すと accesskit が `TreeUpdate includes duplicate child` で
    /// **panic し、窓ごと落ちる**（2026-09-21 に実機で踏んだ：menu を開いた状態で
    /// 読み上げが起きていた）。置き場は `declarative::OVERLAY_INDEX_BASE` で分けたが、
    /// ここは取りこぼしても窓が死なないための受け皿。
    #[test]
    fn two_regions_that_share_a_number_do_not_take_the_window_down() {
        let root = div()
            .w_full()
            .h_full()
            .flex_col()
            .children([button("保存").id("save").w(Px(80.0)).h(Px(32.0))]);
        let mut build = build_tree(&root, 800.0, 600.0);
        // menu の行が混ざった形を作る ── 番号は overlay 側の振り直しでぶつかる。
        // `HitRegion` は Clone ではないので、同じ木をもう一度組んで足す
        // （overlay が別のツリーとして組まれるのと同じ形）。
        let again = build_tree(&root, 800.0, 600.0);
        build.hit_regions.extend(again.hit_regions);

        let t = tree_update(&build, "テスト", None, 2.0);

        let root_node = &t.update.nodes.last().expect("根が無い").1;
        let kids: Vec<_> = root_node.children().to_vec();
        let mut uniq = kids.clone();
        uniq.sort_by_key(|n| n.0);
        uniq.dedup();
        assert_eq!(kids.len(), uniq.len(), "同じ子が 2 つ並んだ TreeUpdate を作った");
    }
}
