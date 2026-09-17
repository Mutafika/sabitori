// web の ⌘C / ⌘X / ⌘V が効くか (#76)。
//
//   node drive.mjs http://127.0.0.1:8099/index.html /tmp/last.png probes/clipboard.mjs
//
// クリップボードの中身は CDP からは直接読めないので、**貼り戻して**確かめる:
// 「打つ → 全選択 → ⌘X → 欄が空になる → ⌘V → 戻る」が通れば、書けていて
// なおかつ消えている (issue #33 の「書けてから消す」もこの形で見ている)。
import fs from "node:fs";

export default async ({ send, sleep, logs }) => {
  const shot = async (name) => {
    const s = await send("Page.captureScreenshot", { format: "png" });
    fs.writeFileSync(`/tmp/${name}.png`, Buffer.from(s.data, "base64"));
    logs.push(`shot: /tmp/${name}.png`);
  };
  const at = async (x, y, type) =>
    send("Input.dispatchMouseEvent", {
      type, x, y, button: "left", clickCount: 1, buttons: type === "mousePressed" ? 1 : 0,
    });
  // macOS の ⌘ は modifiers: 4 (Meta)。
  //
  // **`commands` は CDP の都合。** 合成したキーイベントはブラウザの編集コマンド
  // (cut / copy / paste) を起こさない — 実際のユーザーの打鍵だけが起こす。
  // 渡さないと `cut` / `copy` イベントが飛ばず、「橋渡しが壊れている」ように
  // 見える。実機では要らない。
  const chord = async (k, code, vk, commands = []) => {
    for (const type of ["keyDown", "keyUp"]) {
      await send("Input.dispatchKeyEvent", {
        type, key: k, code, windowsVirtualKeyCode: vk, modifiers: 4,
        commands: type === "keyDown" ? commands : [],
      });
    }
    await sleep(250);
  };

  const probe = async (label) => {
    const r = await send("Runtime.evaluate", {
      expression: `[document.activeElement && document.activeElement.id,
                    document.getElementById('sabitori-ime')?.value].join(' | ')`,
      returnByValue: true,
    });
    logs.push(`${label}: ${r.result.value}`);
  };

  const [fx, fy] = [160, 144];
  await at(fx, fy, "mousePressed");
  await at(fx, fy, "mouseReleased");
  await sleep(400);
  await probe("after click");

  await send("Input.insertText", { text: "予約番号R-0042" });
  await sleep(300);
  await probe("after type");
  await shot("clip_typed");

  await chord("a", "KeyA", 65);   // 全選択
  await sleep(200);
  await probe("after select-all");
  await shot("clip_selected");

  await chord("x", "KeyX", 88, ["cut"]);   // 切り取り
  await sleep(400);
  await probe("after cut");
  await shot("clip_after_cut");

  await chord("v", "KeyV", 86, ["paste"]);   // 貼り付け
  await sleep(400);
  await probe("after paste");
  await shot("clip_after_paste");
};
