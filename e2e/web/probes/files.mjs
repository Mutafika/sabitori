// ファイル保存 (ダウンロード) が始まるか (#77)。
//
//   node drive.mjs http://127.0.0.1:8099/index.html /tmp/last.png probes/files.mjs
//
// CDP の Browser.setDownloadBehavior で保存先を決め、ダウンロードイベントを
// 拾う。中身まで見たいので、保存されたファイルを読む。
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

export default async ({ send, sleep, logs }) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "sabitori-dl-"));
  await send("Browser.setDownloadBehavior", {
    behavior: "allow",
    downloadPath: dir,
    eventsEnabled: true,
  });

  // 「CSV を保存」ボタン (chart 120 + 欄 46 の下の行)
  for (const type of ["mousePressed", "mouseReleased"]) {
    await send("Input.dispatchMouseEvent", {
      type, x: 50, y: 200, button: "left", clickCount: 1,
      buttons: type === "mousePressed" ? 1 : 0,
    });
  }
  await sleep(1200);

  const files = fs.readdirSync(dir);
  logs.push(`ダウンロード: ${JSON.stringify(files)}`);
  for (const f of files) {
    const body = fs.readFileSync(path.join(dir, f), "utf8");
    logs.push(`  ${f}: ${JSON.stringify(body)}`);
  }
};
