// GPU が用意できないときに、理由が画面 (DOM) に出るか (#82)。
//
// WebGL を切った Chrome で開き、canvas が真っ白なまま終わらないことを見る。
//   node drive.mjs http://127.0.0.1:8099/index.html /tmp/nogpu.png probes/gpu_failure.mjs
export default async ({ send, sleep, logs }) => {
  await sleep(2500);
  const r = await send("Runtime.evaluate", {
    expression: `(() => {
      const el = document.getElementById('sabitori-error');
      return JSON.stringify({
        shown: !!el,
        text: el ? el.textContent : null,
        canvas: !!document.getElementById('sabitori-canvas'),
      });
    })()`,
    returnByValue: true,
  });
  const got = JSON.parse(r.result.value);
  logs.push(`error_box=${JSON.stringify(got)}`);
  if (!got.shown) {
    throw new Error("GPU が無いのに画面へ何も出ていない (canvas が真っ白なまま)");
  }
  if (!/画面を表示できませんでした/.test(got.text)) {
    throw new Error(`出ている文言が違う: ${got.text}`);
  }
};
