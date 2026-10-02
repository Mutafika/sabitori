// 縮めて読んだ画像が出るか・読み込みで画面が止まらないか (#114)。
//
//   node drive.mjs http://127.0.0.1:8099/index.html /tmp/thumb.png probes/images.mjs
//
// console の `thumb=WxH` が縮めた大きさ (60 論理 px × dpr の長辺、縦長)。
// `longtasks` は読み込み中に出た 50ms 超えの長いタスク。dist/photo.jpg を
// 4032×3024 の写真に差し替えて回すと、画面のスレッドで読んでいないかが分かる。
export default async ({ send, sleep, logs }) => {
  // 最初のフレームで読み込みが始まるので、計測は読み込みより先に仕掛けて
  // 読み直す。
  await send("Page.addScriptToEvaluateOnNewDocument", {
    source: `window.__long = [];
      new PerformanceObserver((l) => {
        for (const e of l.getEntries()) window.__long.push(Math.round(e.duration));
      }).observe({ type: "longtask", buffered: true });`,
  });
  await send("Page.reload");
  await sleep(8000);
  const r = await send("Runtime.evaluate", {
    expression: "JSON.stringify(window.__long)",
    returnByValue: true,
  });
  logs.push(`longtasks=${r.result.value}`);
};
