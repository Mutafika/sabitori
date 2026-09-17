// 素の打鍵で英数字が入るか (#81)。
//
//   node drive.mjs http://127.0.0.1:8099/index.html /tmp/last.png probes/typing.mjs
//
// **`Input.insertText` は使わない。** あれは keydown を経由しないので、
// 「keydown を prevent_default していて文字がどこにも行かない」形
// (v0.12.0 の web) をすり抜けてしまう。ここは本物の打鍵だけを送る。
//
// 欄の中身は canvas なので読めない。アプリ側が変更のたび console に
// `name=...` を出しているので、それを突き合わせる。
export default async ({ send, sleep, logs }) => {
  const at = async (x, y, type) =>
    send("Input.dispatchMouseEvent", {
      type, x, y, button: "left", clickCount: 1, buttons: type === "mousePressed" ? 1 : 0,
    });
  // 本物の打鍵 (text を載せると Chrome が char イベントまで出す = 実機と同じ)。
  const type_ = async (ch, code, vk, modifiers = 0) => {
    await send("Input.dispatchKeyEvent", {
      type: "keyDown", key: ch, code, text: ch, unmodifiedText: ch,
      windowsVirtualKeyCode: vk, modifiers,
    });
    await send("Input.dispatchKeyEvent", {
      type: "keyUp", key: ch, code, windowsVirtualKeyCode: vk, modifiers,
    });
    await sleep(90);
  };
  const press = async (key, code, vk, modifiers = 0) => {
    for (const t of ["keyDown", "keyUp"]) {
      await send("Input.dispatchKeyEvent", { type: t, key, code, windowsVirtualKeyCode: vk, modifiers });
    }
    await sleep(90);
  };
  const lastName = () => {
    const hit = [...logs].reverse().find((l) => l.includes("name="));
    // **trim しない** — 末尾の空白そのものを見たい (スペースが入ったか)。
    return hit ? hit.slice(hit.indexOf("name=") + 5).replace(/[\r\n]+$/, "") : null;
  };

  // 欄を押して焦点を当てる。
  const [fx, fy] = [160, 144];
  await at(fx, fy, "mousePressed");
  await at(fx, fy, "mouseReleased");
  await sleep(500);

  // 1) 小文字
  await type_("a", "KeyA", 65);
  await type_("b", "KeyB", 66);
  await type_("c", "KeyC", 67);
  // 2) 大文字 (Shift)
  await type_("D", "KeyD", 68, 8);
  // 3) 数字と記号 (パスワードで使う)
  await type_("4", "Digit4", 52);
  await type_("@", "Digit2", 50, 8);
  await sleep(300);

  const typed = lastName();
  logs.push(`typed=${JSON.stringify(typed)}`);
  if (typed !== "abcD4@") {
    throw new Error(`素の打鍵が届いていない: name=${JSON.stringify(typed)} (期待 "abcD4@")`);
  }

  // 4) Backspace はこれまでどおり効く (印字可能キーを通した副作用で壊れやすい)
  await press("Backspace", "Backspace", 8);
  await sleep(250);
  if (lastName() !== "abcD4") {
    throw new Error(`Backspace が効いていない: ${JSON.stringify(lastName())}`);
  }

  // 5) スペースでページが動かず、文字として入ること
  await type_(" ", "Space", 32);
  await sleep(250);
  if (lastName() !== "abcD4 ") {
    throw new Error(`スペースが入っていない: ${JSON.stringify(lastName())}`);
  }

  logs.push("typing: ok");
};
