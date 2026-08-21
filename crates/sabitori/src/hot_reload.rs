//! Rust コードのホットリロード (dev 専用、`feature = "hot-reload"`)。
//!
//! 走っているプロセスに機械語パッチを撃ち込む [subsecond] の受け口。パッチを作る側
//! — リンカを乗っ取り、再コンパイル前後のアセンブリ差分からシンボルを差し替える部分 —
//! は Dioxus CLI が持っているので、起動は `cargo run` ではなく `dx serve --hotpatch`
//! になる。CLI が入っていない / devserver が居ない場合は黙って素通りし、アプリは
//! 通常どおり起動する。
//!
//! ## なぜ `dioxus-devtools` を使わないか
//!
//! 上流にも同じことをする [`dioxus_devtools::connect_subsecond`] があるが、あれは
//! dioxus-core と dioxus-signals (VirtualDom へのテンプレート適用) を必ず連れてくる。
//! Sabitori に VirtualDom は無く、欲しいのは devserver が流してくる jump table
//! ひとつだけなので、WebSocket を自前で読む。依存は subsecond / tungstenite /
//! serde_json / dioxus-cli-config の 4 本に収まり、Dioxus 本体は入らない。
//!
//! ## 何がリロードされて、何がされないか
//!
//! - **される**: [`DeclarativeApp::view`](crate::DeclarativeApp::view) の中身と、
//!   そこから呼ばれる全て。レイアウト・色・文言・分岐。状態は保持される。
//! - **されない**: アプリの状態を持つ struct のフィールド追加・削除・型変更。
//!   メモリレイアウトが変わるため、これをやったら `dx` 側がフル再起動に落とす。
//!
//! subsecond は `debug_assertions` が有効なときだけ働く。release ビルドでは
//! [`call`] は素の呼び出しに畳まれるので、feature を立てたまま出荷しても実行時
//! コストは無い。
//!
//! [subsecond]: https://crates.io/crates/subsecond

/// ホットリロードの境界。
///
/// ここより内側で呼ばれる関数はパッチ後の新しい実装に差し替わる。境界が要るのは
/// 「今まさにスタックに載っている関数のコードが差し替わった」場合に、安全な地点まで
/// 巻き戻す必要があるため。毎フレーム抜ける `view()` はその地点として理想的で、
/// 逆に一度きりしか通らない初期化を包んでも意味がない。
///
/// feature が off、または WASM ターゲットでは `f()` をそのまま呼ぶだけ。
#[inline]
pub fn call<O>(f: impl FnMut() -> O) -> O {
    imp::call(f)
}

/// devserver への接続を開き、パッチが当たるたびに `on_patch` を呼ぶ。
///
/// `on_patch` は WebSocket を読んでいる別スレッドから呼ばれる。ここで直接描画は
/// できないので、イベントループを叩き起こして再描画を要求する用途に使う
/// (`run_declarative` は `EventLoopProxy` を送る)。
///
/// devserver が居なければ何もしない。ホットリロードなしで普通に動く。
#[inline]
pub fn init(on_patch: impl Fn() + Send + Sync + 'static) {
    imp::init(on_patch);
}

#[cfg(all(feature = "hot-reload", not(target_arch = "wasm32")))]
mod imp {
    use std::sync::Arc;

    pub fn call<O>(f: impl FnMut() -> O) -> O {
        subsecond::call(f)
    }

    pub fn init(on_patch: impl Fn() + Send + Sync + 'static) {
        subsecond::register_handler(Arc::new(on_patch));

        let Some(endpoint) = dioxus_cli_config::devserver_ws_endpoint() else {
            tracing::info!(
                "hot-reload: devserver が見つからないので無効。\
                 有効にするには `dx serve --hotpatch` で起動する"
            );
            return;
        };

        // aslr_reference は「このプロセスの main がどこに載ったか」。ASLR で毎回
        // ずれるので、パッチ側がシンボルを解決するには実行中プロセスから教える
        // しかない。接続 URL に載せるのが devserver プロトコルの作法。
        let uri = format!(
            "{endpoint}?aslr_reference={}&build_id={}&pid={}",
            subsecond::aslr_reference(),
            dioxus_cli_config::build_id(),
            std::process::id(),
        );

        let spawned = std::thread::Builder::new()
            .name("sabitori-hot-reload".into())
            .spawn(move || {
                let mut ws = match tungstenite::connect(&uri) {
                    Ok((ws, _)) => ws,
                    Err(e) => {
                        tracing::warn!("hot-reload: devserver に接続できない: {e}");
                        return;
                    }
                };
                tracing::info!("hot-reload: devserver に接続した");
                while let Ok(msg) = ws.read() {
                    if let tungstenite::Message::Text(text) = msg {
                        apply(&text);
                    }
                }
                tracing::info!("hot-reload: devserver との接続が切れた");
            });
        if let Err(e) = spawned {
            tracing::warn!("hot-reload: 受信スレッドを起動できない: {e}");
        }
    }

    /// devserver のメッセージから jump table だけ取り出して適用する。
    fn apply(text: &str) {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(text) else {
            return;
        };
        // 外部タグ付き enum: `{"HotReload": {..}}`。他の variant (FullReloadCommand,
        // Shutdown など) は VirtualDom を持つアプリ向けなので黙って捨てる。Value 経由
        // で読むのは、上流が variant やフィールドを足しても壊れないようにするため。
        let Some(hot) = msg.get("HotReload") else {
            return;
        };
        // 一つの devserver に複数プロセスがぶら下がることがある。自分宛て以外を
        // 当てると他プロセス向けのコードを踏むので、pid 一致は必須。
        if hot.get("for_pid").and_then(serde_json::Value::as_u64) != Some(u64::from(std::process::id())) {
            return;
        }
        // テンプレートだけ変わった (Rust コードは無傷の) 通知には jump table が無い。
        let Some(table) = hot.get("jump_table").filter(|t| !t.is_null()) else {
            return;
        };
        let table: subsecond::JumpTable = match serde_json::from_value(table.clone()) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("hot-reload: jump table を読めない: {e}");
                return;
            }
        };

        // SAFETY: table は dx が「今このプロセスで走っているバイナリ」向けに吐き、
        // pid 一致で自分宛てだと確認済みのもの。適用に成功すると subsecond が
        // `init` で登録したハンドラを呼び、再描画が要求される。
        if let Err(e) = unsafe { subsecond::apply_patch(table) } {
            tracing::warn!("hot-reload: パッチ適用に失敗: {e}");
        }
    }
}

#[cfg(not(all(feature = "hot-reload", not(target_arch = "wasm32"))))]
mod imp {
    #[inline(always)]
    pub fn call<O>(mut f: impl FnMut() -> O) -> O {
        f()
    }

    #[inline(always)]
    pub fn init(_on_patch: impl Fn() + Send + Sync + 'static) {}
}
