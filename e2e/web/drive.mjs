// 依存ゼロの CDP ドライバ。Node 22+ の global WebSocket を使う。
// 使い方: node cdp.mjs <url> <out.png> [script.mjs]
const [url, out, extra] = process.argv.slice(2);

async function cdpTarget() {
  for (let i = 0; i < 60; i++) {
    try {
      const r = await fetch("http://127.0.0.1:9222/json/list");
      const list = await r.json();
      const page = list.find((t) => t.type === "page");
      if (page?.webSocketDebuggerUrl) return page.webSocketDebuggerUrl;
    } catch {}
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error("Chrome の CDP に繋がらない");
}

const wsUrl = await cdpTarget();
const ws = new WebSocket(wsUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });

let nextId = 1;
const pending = new Map();
const events = [];
ws.onmessage = (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id && pending.has(msg.id)) {
    const { res, rej } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? rej(new Error(JSON.stringify(msg.error))) : res(msg.result);
  } else if (msg.method) {
    events.push(msg);
  }
};
const send = (method, params = {}) =>
  new Promise((res, rej) => {
    const id = nextId++;
    pending.set(id, { res, rej });
    ws.send(JSON.stringify({ id, method, params }));
  });

const logs = [];
const errors = [];
ws.addEventListener("message", (m) => {
  const msg = JSON.parse(m.data);
  if (msg.method === "Runtime.consoleAPICalled") {
    logs.push(msg.params.args.map((a) => a.value ?? a.description ?? "").join(" "));
  }
  if (msg.method === "Runtime.exceptionThrown") {
    errors.push(msg.params.exceptionDetails.exception?.description
      ?? msg.params.exceptionDetails.text);
  }
  if (msg.method === "Log.entryAdded") {
    logs.push(`[${msg.params.entry.level}] ${msg.params.entry.text}`);
  }
});

await send("Page.enable");
await send("Runtime.enable");
await send("Log.enable");
await send("Page.navigate", { url });
await new Promise((r) => setTimeout(r, 8000));

const globals = { send, logs, errors, sleep: (ms) => new Promise((r) => setTimeout(r, ms)) };
if (extra) {
  // 相対パスをそのまま import() するとパッケージ名として解決されるので、
  // 必ず file:// に直す (`probes/ime.mjs` で ERR_MODULE_NOT_FOUND になる)。
  const { pathToFileURL } = await import("node:url");
  const { resolve } = await import("node:path");
  const mod = await import(pathToFileURL(resolve(extra)).href);
  await mod.default(globals);
}

const shot = await send("Page.captureScreenshot", { format: "png" });
const fs = await import("node:fs");
fs.writeFileSync(out, Buffer.from(shot.data, "base64"));

console.log("--- console ---");
for (const l of logs) console.log(l);
console.log("--- exceptions ---");
for (const e of errors) console.log(e);
console.log(`--- screenshot: ${out} ---`);
ws.close();
process.exit(errors.length ? 1 : 0);
