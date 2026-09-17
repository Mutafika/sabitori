// 日本語 IME・編集キーが web で届くか (#73)。
//
//   node drive.mjs http://127.0.0.1:8099/index.html /tmp/last.png probes/ime.mjs
//
// CDP の Input.imeSetComposition は本物の変換セッションを張るので、隠し
// textarea の compositionupdate / compositionend がそのまま出る。
// 出てくる PNG を見ること — 「変換中の文字が欄に見える」は assert できない。
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
  const key = async (k, code, opts = {}) => {
    for (const type of ["keyDown", "keyUp"]) {
      await send("Input.dispatchKeyEvent", { type, key: k, code, windowsVirtualKeyCode: opts.vk ?? 0, ...opts });
    }
    await sleep(120);
  };

  const [fx, fy] = [160, 144];
  await at(fx, fy, "mousePressed");
  await at(fx, fy, "mouseReleased");
  await sleep(500);

  // 1) 変換中 (preedit) が欄に出るか
  await send("Input.imeSetComposition", { text: "にほんご", selectionStart: 4, selectionEnd: 4 });
  await sleep(400);
  await shot("ime_composing");

  // 2) 確定
  await send("Input.insertText", { text: "日本語" });
  await sleep(400);

  // 3) 半角も入るか
  await send("Input.insertText", { text: "abc" });
  await sleep(300);
  await shot("ime_committed");

  // 4) Backspace が効くか
  await key("Backspace", "Backspace", { vk: 8 });
  await key("Backspace", "Backspace", { vk: 8 });
  await sleep(300);
  await shot("ime_backspace");

  // 5) 矢印 + Shift で選択が動くか (画面で見える)
  await key("ArrowLeft", "ArrowLeft", { vk: 37, modifiers: 8 }); // shift
  await key("ArrowLeft", "ArrowLeft", { vk: 37, modifiers: 8 });
  await sleep(300);
  await shot("ime_select");
};
