//! 作例の引数と書き出し先。
//!
//! - `--out <dir>`: 書き出し先。無ければビルドの target の下の `reel/`
//!   (ふつうは `target/reel/`、`CARGO_TARGET_DIR` を変えていればその下)。
//! - 数字: 動画の代わりに、そのコマだけ PNG で書き出す。

// 作例ごとに使う物が違う (stage3d は動画を書かない)。
#![allow(dead_code)]

use std::path::PathBuf;

pub struct Args {
    pub out: PathBuf,
    pub stills: Vec<u32>,
}

impl Args {
    pub fn parse() -> Self {
        let mut out = None;
        let mut stills = Vec::new();
        let mut args = std::env::args().skip(1);
        while let Some(a) = args.next() {
            if a == "--out" {
                out = args.next().map(PathBuf::from);
            } else if let Ok(n) = a.parse() {
                stills.push(n);
            } else {
                eprintln!("unknown argument: {a} (expected `--out <dir>` or frame numbers)");
                std::process::exit(2);
            }
        }
        Self {
            out: out.unwrap_or_else(default_out),
            stills,
        }
    }

    /// `<out>/<name>.mp4`
    pub fn mp4(&self, name: &str) -> PathBuf {
        self.out.join(format!("{name}.mp4"))
    }

    /// `<out>/<name>/` — PNG を並べる所。
    pub fn frames_dir(&self, name: &str) -> PathBuf {
        self.out.join(name)
    }
}

/// 作例の実行ファイルは `<target>/<profile>/examples/<name>` にあるので、
/// そこから `<target>/reel` を引く。
fn default_out() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.parent()?.join("reel")))
        .unwrap_or_else(|| PathBuf::from("reel"))
}
