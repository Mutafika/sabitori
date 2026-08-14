//! sabitori の高水準ウィジェット。
//!
//! # 規約: **状態は struct、 見た目は自由関数**
//!
//! ウィジェットは 2 つに分かれる。
//!
//! * **状態** — `TextInputState` / `TableState` / `DropdownState` のように、
//!   値と操作だけを持つ struct。 `App` のフィールドに置く。
//! * **見た目** — `text_input(ctx, id, &state, &style) -> Element` のような
//!   自由関数。 `view()` から呼んで、 返ってきた [`Element`](sabitori_core::Element)
//!   をツリーに繋ぐ。
//!
//! **Element を返す入口は必ず `snake_case` の自由関数**で、 第 1 引数が
//! `&ViewContext`、 第 2 引数が `id` になっている。 `sabitori_core::forms` の
//! `checkbox` / `slider` / `radio` / `segment_control` も同じ形なので、
//! 「どうやって Element にするのか」を毎回調べ直さなくていい。
//!
//! ```ignore
//! fn view(&self, ctx: &ViewContext) -> Element {
//!     div().flex_col().children([
//!         text_input(ctx, "name", &self.name, &TextInputStyle::default_dark()),
//!         table(ctx, "files", &self.files, &TableStyle::default_dark()),
//!     ])
//! }
//! ```
//!
//! # 0.4.0 で消したもの
//!
//! `Button` / `Card` / `Tabs` / `Table` / `SplitPane` / `Dropdown` / `TextInput`
//! には、 `new(x, y, w, h)` や `new(bounds: Rect)` に**画面座標を渡す**古い版が
//! あった。 あれは `Element` を返さず、 当たり判定も `hit_test(point)` で自分で
//! やる前提の retained 型で、 `view()` からは使えなかった (repo 内の使用箇所も
//! 0 だった)。 それぞれ以下に置き換わっている。
//!
//! | 消えたもの | 代わり |
//! |---|---|
//! | `Button` | `sabitori_core::element::button()` |
//! | `Card` | `div()` + `.bg()` / `.rounded()` / `.shadow_md()` |
//! | `Tabs` | `sabitori_core::forms::segment_control()` |
//! | `Dropdown` | [`DropdownState`] + [`DropdownStyle`] |
//! | `TextInput` | [`text_input`] + [`TextInputState`]、折り返す欄は [`text_area`] |
//! | `Table` | [`table`] + [`TableState`] (宣言版に作り直し) |
//! | `SplitPane` | [`split_pane`] + [`SplitPaneState`] (宣言版に作り直し) |

mod color_picker;
mod context_menu_widget;
mod date_picker;
mod dock_group;
mod drag;
mod focus;
mod menu_bar;
mod modal;
mod numeric_input;
mod panel;
mod presence;
mod scroll_view;
mod select;
mod slider;
mod snap;
mod split_pane;
mod style_animator;
mod table;
mod text_input;
mod toast;
mod tooltip;
mod tree_view;
mod virtual_list;
mod window_drag;
pub mod file_browser;

pub use color_picker::{ColorPickerState, ColorPickerStyle};
pub use context_menu_widget::{ContextMenuState, MenuItemDef};
pub use date_picker::{DatePickerState, DatePickerStyle};
pub use dock_group::{drop_split, DockAxis, DockGroup, MIN_PANE_PX, SPLITTER_PX};
pub use drag::DragManager;
pub use focus::{FocusChange, FocusKeyResult, FocusManager};
pub use menu_bar::{MenuBarState, MenuBarStyle, MenuDef};
pub use modal::{Modal, ModalStyle};
pub use numeric_input::NumericInputState;
pub use panel::{Panel, PanelSide};
pub use presence::PresenceAnimator;
pub use scroll_view::ScrollView;
pub use select::{DropdownEvent, DropdownState, DropdownStyle};
pub use slider::SliderState;
pub use snap::{snap_rect, SnapGuides};
pub use split_pane::{
    split_pane, SplitDirection, SplitPaneState, SplitPaneStyle, DIVIDER_PX,
};
pub use style_animator::StyleAnimator;
pub use table::{
    table, table_clicked_header, table_clicked_row, table_header_id, table_row_id, Cell,
    TableColumn, TableState, TableStyle,
};
pub use text_input::{
    text_area, text_input, PendingMove, PreeditState, TextInputState, TextInputStyle,
};
pub use toast::{ToastKind, ToastManager};
pub use tooltip::TooltipState;
pub use tree_view::{
    tree_clicked_row, tree_row_id, tree_view, FlatTreeItem, TreeNode, TreeViewStyle,
};
pub use virtual_list::{virtual_list, VirtualList};
pub use window_drag::WindowDragState;
