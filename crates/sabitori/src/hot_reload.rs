//! Rust コードのホットリロード (dev 専用、`feature = "hot-reload"`)。
//!
//! 走っているプロセスに機械語パッチを撃ち込む [subsecond] の受け口。パッチを作る側
//! — リンカを乗っ取り、再コンパイル前後のアセンブリ差分からシンボルを差し替える部分 —
//! は Dioxus CLI が持っているので、起動は `cargo run` ではなく `dx serve --hot-patch`
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
                 有効にするには `dx serve --hot-patch` で起動する"
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
        let Some(table) = patch_for(text, std::process::id()) else {
            return;
        };
        let table: subsecond::JumpTable = match serde_json::from_value(table) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("hot-reload: jump table を読めない: {e}");
                return;
            }
        };

        // SAFETY: table は dx が「今このプロセスで走っているバイナリ」向けに吐き、
        // pid 一致で自分宛てだと確認済みのもの。適用に成功すると subsecond が
        // `init` で登録したハンドラを呼び、再描画が要求される。
        match unsafe { subsecond::apply_patch(table) } {
            // 成功を黙っていると、効いているのか実は落ちているのかが区別できない。
            // dev 用の機能なので、当たったことは必ず言う。
            Ok(()) => tracing::info!("hot-reload: パッチ適用"),
            Err(e) => tracing::warn!("hot-reload: パッチ適用に失敗: {e}"),
        }
    }

    /// devserver の 1 メッセージから、**自分が当てるべき** jump table を取り出す。
    ///
    /// 当てないと決める経路が 4 本あり、どれも黙って `None` を返す。
    /// 「dx はパッチを作ったと言っているのに画面が変わらない」はここで起きるので、
    /// 判断だけを `unsafe` から切り離してテストできるようにしてある。
    fn patch_for(text: &str, our_pid: u32) -> Option<serde_json::Value> {
        let msg: serde_json::Value = serde_json::from_str(text).ok()?;
        // 外部タグ付き enum: `{"HotReload": {..}}`。他の variant (FullReloadCommand,
        // Shutdown など) は VirtualDom を持つアプリ向けなので黙って捨てる。Value 経由
        // で読むのは、上流が variant やフィールドを足しても壊れないようにするため。
        let hot = msg.get("HotReload")?;
        // 一つの devserver に複数プロセスがぶら下がることがある。自分宛て以外を
        // 当てると他プロセス向けの機械語を自分に撃ち込むことになるので、pid 一致は必須。
        let for_pid = hot.get("for_pid").and_then(serde_json::Value::as_u64);
        if for_pid != Some(u64::from(our_pid)) {
            tracing::debug!(
                "hot-reload: 自分宛てでないパッチを無視 (for_pid={for_pid:?}, self={our_pid})"
            );
            return None;
        }
        // テンプレートだけ変わった (Rust コードは無傷の) 通知には jump table が無い。
        hot.get("jump_table").filter(|t| !t.is_null()).cloned()
    }

    #[cfg(test)]
    mod tests {
        use super::patch_for;

        const PID: u32 = 4242;

        fn msg(for_pid: &str, jump_table: &str) -> String {
            format!(
                r#"{{"HotReload":{{"templates":[],"assets":[],"ms_elapsed":12,
                   "jump_table":{jump_table},"for_build_id":0,"for_pid":{for_pid}}}}}"#
            )
        }

        #[test]
        fn a_patch_addressed_to_us_is_taken() {
            assert!(patch_for(&msg("4242", r#"{"map":{}}"#), PID).is_some());
        }

        /// **同じ devserver にぶら下がる別プロセス宛て。**
        /// 当ててしまうと、他プロセス向けの機械語を自分に撃ち込むことになる。
        #[test]
        fn a_patch_for_another_pid_is_dropped() {
            assert!(patch_for(&msg("9999", r#"{"map":{}}"#), PID).is_none());
        }

        /// 宛先が書かれていないものも当てない (wasm 向けは `null` で来る)。
        #[test]
        fn a_patch_with_no_pid_is_dropped() {
            assert!(patch_for(&msg("null", r#"{"map":{}}"#), PID).is_none());
        }

        /// Rust コードが無傷の通知。当てるものが無い。
        #[test]
        fn a_message_without_a_jump_table_is_dropped() {
            assert!(patch_for(&msg("4242", "null"), PID).is_none());
        }

        /// VirtualDom を持つアプリ向けの variant。Sabitori には関係がない。
        #[test]
        fn other_devserver_variants_are_ignored() {
            for m in [r#""Shutdown""#, r#""FullReloadCommand""#, r#"{"HotPatchStart":null}"#] {
                assert!(patch_for(m, PID).is_none(), "{m} を拾ってしまった");
            }
        }

        /// 上流がフィールドを足しても壊れないこと (Value 経由で読んでいる理由)。
        #[test]
        fn unknown_fields_do_not_break_parsing() {
            let m = r#"{"HotReload":{"jump_table":{"map":{}},"for_pid":4242,
                        "some_future_field":{"nested":true}}}"#;
            assert!(patch_for(m, PID).is_some());
        }

        /// 壊れた入力で落ちないこと。
        #[test]
        fn garbage_is_not_a_panic() {
            for m in ["", "not json", "{", "[]", "null"] {
                assert!(patch_for(m, PID).is_none());
            }
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
