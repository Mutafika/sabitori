//! GPU テクスチャの予算管理。
//!
//! [`ImageRenderer`](crate::ImageRenderer) が抱えるテクスチャは、かつて入れる口
//! しか無く、窓を閉じるまで一度も解放されなかった。画像を鍵で切り替えるアプリ
//! — 一覧のサムネイルと原寸プレビュー、鍵に更新時刻を混ぜて作り直しを検知する
//! ような形 — では、画面から消えた画像のテクスチャも全部残り続け、GB 級に達して
//! 戻らなくなる。しかも wgpu のエラーは既定で致命的なので、限界に当たった時の
//! 出方は「絵が出ない」ではなく**窓が落ちる**
//! ([#43](https://github.com/Mutafika/sabitori/issues/43))。
//!
//! ここが持つのは**バイト数と最終使用世代だけ**で、wgpu には触らない。
//! 「どれを追い出すか」の判断はこの型の中で完結するので、GPU を用意せずに
//! 検査できる。実際にテクスチャを捨てるのは呼び出し側。
//!
//! ## 枚数ではなくバイト数で測る理由
//!
//! 240px のサムネイルは 1 枚 ≒ 230KB、832×1216 の原寸は ≒ 4MB で、**18 倍**違う。
//! 枚数で上限を切ると、サムネイルばかりの時は緩すぎ、原寸を並べた時は効かない。

use std::collections::HashMap;

/// 1 枚のテクスチャについて覚えておくこと。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Entry {
    bytes: usize,
    /// 最後に使われた世代。[`TextureBudget::end_frame`] で世代が進む。
    last_used: u64,
}

/// テクスチャ群のバイト予算と LRU の帳簿。
#[derive(Debug)]
pub struct TextureBudget {
    budget_bytes: usize,
    used_bytes: usize,
    generation: u64,
    entries: HashMap<String, Entry>,
}

impl TextureBudget {
    /// 既定の予算 (256 MiB)。
    ///
    /// 832×1216 の原寸プレビューで約 64 枚、240px のサムネイルなら約 1100 枚に
    /// あたる。まともな使い方なら当たらず、際限なく漏らす経路だけを塞ぐ高さ。
    pub const DEFAULT_BUDGET_BYTES: usize = 256 * 1024 * 1024;

    pub fn new(budget_bytes: usize) -> Self {
        Self {
            budget_bytes,
            used_bytes: 0,
            generation: 0,
            entries: HashMap::new(),
        }
    }

    /// 予算を変える。縮めた場合、次の [`admit`](Self::admit) で追い出しが起きる。
    pub fn set_budget_bytes(&mut self, bytes: usize) {
        self.budget_bytes = bytes;
    }

    pub fn budget_bytes(&self) -> usize {
        self.budget_bytes
    }

    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// `key` を今の世代で使ったことにする。
    ///
    /// 既に入っている鍵に対して呼ぶ。入っていなければ何もしない。
    pub fn touch(&mut self, key: &str) {
        let now = self.generation;
        if let Some(e) = self.entries.get_mut(key) {
            e.last_used = now;
        }
    }

    /// このフレームの描画が終わったことにして、世代を進める。
    ///
    /// これを呼ばないと「今フレーム使った物」と「前に使った物」が区別できず、
    /// **今から描く物を追い出してしまう**。
    pub fn end_frame(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// `bytes` の新しいテクスチャを入れる前に、**追い出すべき鍵**を返す。
    ///
    /// 返るのは古い順。呼び出し側はそれらを実際に捨ててから新しい物を入れる。
    /// 帳簿の側は呼んだ時点で更新済み — 返した鍵はもう入っていない。
    ///
    /// 今の世代で使われた鍵は返さない。**これから描く物を捨てては意味がない**
    /// (捨てた次の瞬間に入れ直すことになり、毎フレーム上げ直しが起きる)。
    /// そのせいで予算に収まらないときは、収まらないまま入れる — 1 フレームぶんの
    /// working set が予算を超えているということで、それは予算の設定が低すぎる。
    /// 描画を壊すより超過を許す。
    pub fn admit(&mut self, key: &str, bytes: usize) -> Vec<String> {
        // 同じ鍵が既にあるなら入れ替え扱い。古いぶんを先に引く。
        if let Some(old) = self.entries.remove(key) {
            self.used_bytes -= old.bytes;
        }

        let mut evicted = Vec::new();
        while self.used_bytes + bytes > self.budget_bytes {
            let Some(victim) = self.lru_victim() else { break };
            let e = self.entries.remove(&victim).expect("lru_victim は entries から選ぶ");
            self.used_bytes -= e.bytes;
            evicted.push(victim);
        }

        self.entries.insert(
            key.to_string(),
            Entry { bytes, last_used: self.generation },
        );
        self.used_bytes += bytes;
        evicted
    }

    /// 今の世代で使われていない鍵のうち、最も古い物。
    fn lru_victim(&self) -> Option<String> {
        let now = self.generation;
        self.entries
            .iter()
            .filter(|(_, e)| e.last_used != now)
            // (last_used, key) で並べる。key まで見るのは、同世代が並んだときに
            // HashMap の反復順で結果が変わらないようにするため。
            .min_by(|a, b| a.1.last_used.cmp(&b.1.last_used).then_with(|| a.0.cmp(b.0)))
            .map(|(k, _)| k.clone())
    }

    /// `key` を帳簿から外す。入っていれば `true`。
    pub fn remove(&mut self, key: &str) -> bool {
        match self.entries.remove(key) {
            Some(e) => {
                self.used_bytes -= e.bytes;
                true
            }
            None => false,
        }
    }

    /// `f` が `false` を返した鍵を外し、外した鍵を返す。
    pub fn retain(&mut self, mut f: impl FnMut(&str) -> bool) -> Vec<String> {
        let dropped: Vec<String> = self
            .entries
            .keys()
            .filter(|k| !f(k))
            .cloned()
            .collect();
        for k in &dropped {
            self.remove(k);
        }
        dropped
    }

    /// 全部外す。
    pub fn clear(&mut self) {
        self.entries.clear();
        self.used_bytes = 0;
    }
}

impl Default for TextureBudget {
    fn default() -> Self {
        Self::new(Self::DEFAULT_BUDGET_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: usize = 1024 * 1024;

    /// 240px サムネイル相当 (230KB)。
    fn thumb() -> usize {
        240 * 240 * 4
    }

    /// 832×1216 の原寸プレビュー相当 (4MB)。
    fn full() -> usize {
        832 * 1216 * 4
    }

    #[test]
    fn an_empty_budget_uses_nothing() {
        let b = TextureBudget::new(10 * MB);
        assert_eq!(b.used_bytes(), 0);
        assert_eq!(b.len(), 0);
        assert!(b.is_empty());
    }

    #[test]
    fn admitting_within_budget_evicts_nothing() {
        let mut b = TextureBudget::new(10 * MB);
        assert!(b.admit("a", full()).is_empty());
        assert!(b.admit("b", full()).is_empty());
        assert_eq!(b.len(), 2);
        assert_eq!(b.used_bytes(), 2 * full());
    }

    /// **報告された形。** 原寸プレビューをめくり続けても、予算で頭打ちになる。
    #[test]
    fn paging_through_large_images_stays_within_budget() {
        let mut b = TextureBudget::new(10 * MB); // 原寸 2 枚ぶん
        for i in 0..1000 {
            b.admit(&format!("page-{i}"), full());
            b.end_frame();
            assert!(
                b.used_bytes() <= 10 * MB,
                "{i} 枚目で予算を超えた: {} bytes",
                b.used_bytes()
            );
        }
        assert!(b.len() <= 3, "際限なく増えていない: {} 枚", b.len());
    }

    /// 追い出されるのは最も古い物。
    #[test]
    fn the_least_recently_used_goes_first() {
        let mut b = TextureBudget::new(3 * full());
        for k in ["a", "b", "c"] {
            b.admit(k, full());
        }
        b.end_frame();
        // a と c だけを使う。b が最も古くなる。
        b.touch("a");
        b.touch("c");
        b.end_frame();

        assert_eq!(b.admit("d", full()), vec!["b".to_string()]);
        assert!(b.contains("a") && b.contains("c") && b.contains("d"));
        assert!(!b.contains("b"));
    }

    /// **今フレーム使っている物は追い出さない。**
    ///
    /// ここが緩むと、これから描く画像を捨てては入れ直す往復が毎フレーム起きる。
    #[test]
    fn images_used_this_frame_are_never_evicted() {
        let mut b = TextureBudget::new(2 * full());
        b.admit("a", full());
        b.admit("b", full());
        // 世代を進めずに 3 枚目 = a も b も「今フレーム」の物
        let evicted = b.admit("c", full());
        assert!(evicted.is_empty(), "今フレームの物を捨ててはいけない");
        // 収まらないまま入れる。描画を壊すより超過を許す。
        assert!(b.used_bytes() > b.budget_bytes());
        assert_eq!(b.len(), 3);
    }

    /// サムネイルと原寸が混ざっても、バイト数で測るので破綻しない。
    #[test]
    fn a_byte_budget_handles_mixed_sizes() {
        let mut b = TextureBudget::new(5 * MB);
        for i in 0..20 {
            b.admit(&format!("thumb-{i}"), thumb());
            b.end_frame();
        }
        let after_thumbs = b.len();
        assert!(after_thumbs > 4, "サムネイルなら多く入る: {after_thumbs} 枚");

        b.admit("full-0", full());
        b.end_frame();
        assert!(b.used_bytes() <= 5 * MB);
    }

    /// 同じ鍵で入れ直したら、バイト数は二重計上しない。
    #[test]
    fn re_admitting_the_same_key_replaces_it() {
        let mut b = TextureBudget::new(10 * MB);
        b.admit("a", full());
        b.admit("a", thumb());
        assert_eq!(b.len(), 1);
        assert_eq!(b.used_bytes(), thumb(), "古いぶんが残っていない");
    }

    #[test]
    fn removing_frees_its_bytes() {
        let mut b = TextureBudget::new(10 * MB);
        b.admit("a", full());
        assert!(b.remove("a"));
        assert_eq!(b.used_bytes(), 0);
        assert!(!b.remove("a"), "2 度目は false");
    }

    /// アプリが自分で捨てる形 (提案 2)。
    #[test]
    fn retain_drops_what_the_predicate_rejects() {
        let mut b = TextureBudget::new(100 * MB);
        for k in ["keep-1", "drop-1", "keep-2", "drop-2"] {
            b.admit(k, thumb());
        }
        let mut dropped = b.retain(|k| k.starts_with("keep"));
        dropped.sort();
        assert_eq!(dropped, vec!["drop-1".to_string(), "drop-2".to_string()]);
        assert_eq!(b.len(), 2);
        assert_eq!(b.used_bytes(), 2 * thumb());
    }

    #[test]
    fn clearing_frees_everything() {
        let mut b = TextureBudget::new(100 * MB);
        b.admit("a", full());
        b.admit("b", full());
        b.clear();
        assert!(b.is_empty());
        assert_eq!(b.used_bytes(), 0);
    }

    /// 予算を縮めたら、次の admit で効く。
    #[test]
    fn shrinking_the_budget_takes_effect_on_the_next_admit() {
        let mut b = TextureBudget::new(100 * MB);
        for k in ["a", "b", "c"] {
            b.admit(k, full());
        }
        b.end_frame();
        b.set_budget_bytes(2 * full());
        b.admit("d", full());
        assert!(b.used_bytes() <= 2 * full(), "{} bytes", b.used_bytes());
    }

    /// 追い出しの結果が HashMap の反復順に左右されないこと。
    #[test]
    fn eviction_is_deterministic() {
        let pick = || {
            let mut b = TextureBudget::new(2 * full());
            for k in ["a", "b"] {
                b.admit(k, full());
            }
            b.end_frame();
            b.admit("c", full())
        };
        let first = pick();
        for _ in 0..20 {
            assert_eq!(pick(), first, "実行ごとに追い出す物が変わる");
        }
    }
}
