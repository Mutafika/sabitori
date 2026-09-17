// URL と戻るボタンが効くか (#74)。
//
//   node drive.mjs http://127.0.0.1:8099/index.html /tmp/last.png probes/history.mjs
//
// 「押す → URL が変わる → 戻る → 画面も戻る」を通しで見る。画面が戻ったかは
// PNG で確かめる (一覧 / 詳細 #n の表示)。
import fs from "node:fs";

export default async ({ send, sleep, logs }) => {
  const shot = async (name) => {
    const s = await send("Page.captureScreenshot", { format: "png" });
    fs.writeFileSync(`/tmp/${name}.png`, Buffer.from(s.data, "base64"));
    logs.push(`shot: /tmp/${name}.png`);
  };
  const url = async (label) => {
    const r = await send("Runtime.evaluate", {
      expression: "location.hash + ' | len=' + history.length",
      returnByValue: true,
    });
    logs.push(`${label}: ${r.result.value}`);
    return r.result.value;
  };
  const click = async (x, y) => {
    for (const type of ["mousePressed", "mouseReleased"]) {
      await send("Input.dispatchMouseEvent", {
        type, x, y, button: "left", clickCount: 1,
        buttons: type === "mousePressed" ? 1 : 0,
      });
    }
    await sleep(400);
  };

  await url("起動時");

  // 「詳細をひらく」は chart(120) + 欄(約 46) の下あたり
  await click(60, 200);
  await url("1 回押した");
  await click(60, 200);
  await url("2 回押した");
  await shot("history_detail2");

  // 戻る
  await send("Page.navigateToHistoryEntry", {
    entryId: (await send("Page.getNavigationHistory")).entries.at(-2).id,
  });
  await sleep(700);
  await url("戻った");
  await shot("history_back");
};
