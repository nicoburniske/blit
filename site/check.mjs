import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";

const base = process.argv[2] || "http://127.0.0.1:8766/";
const debuggerUrl = process.env.BLIT_CDP_URL || "http://127.0.0.1:9223";
const target = await fetch(`${debuggerUrl}/json/new?${encodeURIComponent(base)}`, { method: "PUT" }).then(response => response.json());
const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise(resolve => socket.addEventListener("open", resolve, { once: true }));
let serial = 0;
const pending = new Map();
const errors = [];
socket.addEventListener("message", event => {
  const message = JSON.parse(event.data);
  if (message.method === "Runtime.exceptionThrown") errors.push(message.params.exceptionDetails);
  if (!message.id) return;
  const promise = pending.get(message.id);
  pending.delete(message.id);
  if (message.error) promise.reject(new Error(JSON.stringify(message.error)));
  else promise.resolve(message.result);
});
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const id = ++serial;
  pending.set(id, { resolve, reject });
  socket.send(JSON.stringify({ id, method, params }));
});
const evaluate = async expression => {
  const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
};
const settle = () => evaluate("new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
const action = label => `Array.from(document.querySelectorAll('button, a')).find(element => (element.getAttribute('aria-label') || element.textContent) === ${JSON.stringify(label)})`;
const screenshot = async name => {
  await settle();
  const { data } = await send("Page.captureScreenshot", { format: "png" });
  await writeFile(`target/site-preview/${name}.png`, Buffer.from(data, "base64"));
};

try {
  await mkdir("target/site-preview", { recursive: true });
  await send("Runtime.enable");
  await send("Page.enable");
  await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 960, deviceScaleFactor: 1, mobile: false });
  await send("Page.navigate", { url: base });
  const started = Date.now();
  while (!(await evaluate("document.querySelector('#status')?.hidden === true"))) {
    assert(Date.now() - started < 15000, "site failed to start");
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  await settle();
  assert(await evaluate("document.body.scrollHeight > innerHeight"), "document must scroll");
  assert(await evaluate("document.documentElement.scrollWidth <= innerWidth"), "desktop horizontal overflow");
  assert(await evaluate("Boolean(document.querySelector('main h1, main [role=heading][aria-level=\"1\"]'))"), "page has an accessible heading in the main landmark");
  assert(await evaluate(`${action("Overview")}.getAttribute('aria-current') === 'page'`), "current navigation is accessible");
  await screenshot("desktop-overview");

  assert(await evaluate(`Boolean(${action("+ Increment")})`), "increment is accessible");
  await evaluate(`${action("+ Increment")}.scrollIntoView({block: 'center'})`);
  await settle();
  const bounds = await evaluate(`(() => { const area = ${action("+ Increment")}.getBoundingClientRect(); return {x: area.x + area.width/2, y: area.y + area.height/2}; })()`);
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", ...bounds });
  await send("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", clickCount: 1, ...bounds });
  await send("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", clickCount: 1, ...bounds });
  await settle();
  assert(await evaluate("document.body.textContent.includes('001')"), "pointer click updates count");
  await evaluate(`${action("+ Increment")}.focus()`);
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  await settle();
  if (!(await evaluate("document.body.textContent.includes('002')"))) errors.push("keyboard activation did not update count");
  const beforeSpace = await evaluate("window.scrollY");
  await send("Input.dispatchKeyEvent", { type: "keyDown", key: " ", code: "Space", windowsVirtualKeyCode: 32 });
  await send("Input.dispatchKeyEvent", { type: "keyUp", key: " ", code: "Space", windowsVirtualKeyCode: 32 });
  await settle();
  assert(await evaluate("document.body.textContent.includes('003')"), "space activates a focused button");
  assert.equal(await evaluate("window.scrollY"), beforeSpace, "space on a button must not scroll the page");
  await screenshot("desktop-model");
  await evaluate(`${action("Reset")}.click()`);
  await settle();
  assert(await evaluate("document.body.textContent.includes('000')"), "reset updates count");
  await evaluate(`${action("100k")}.scrollIntoView({block: 'center'}); ${action("100k")}.click()`);
  await settle();
  assert(await evaluate("document.body.textContent.includes('2.83 ms')"), "benchmark scale updates");
  assert(await evaluate(`${action("100k")}.getAttribute('aria-pressed') === 'true'`), "selected benchmark scale is accessible");
  await screenshot("desktop-benchmarks");

  await evaluate("location.hash = '#comparisons'");
  await settle();
  assert(await evaluate("document.title === 'Comparisons — Blit'"), "comparison route updates title");
  await screenshot("desktop-comparisons");
  for (const [label, expected] of [["Clay", "4,076"], ["Slint", "111,675"], ["Ratatui", "24,514"], ["egui", "52,689"]]) {
    await evaluate(`${action(label)}.click()`);
    await settle();
    assert(await evaluate(`document.body.textContent.includes(${JSON.stringify(expected)})`), `${label} comparison updates`);
  }
  await evaluate("window.scrollTo(0, document.body.scrollHeight)");
  await screenshot("desktop-comparison-sources");

  for (const width of [390, 320]) {
    await send("Emulation.setDeviceMetricsOverride", { width, height: 844, deviceScaleFactor: 2, mobile: true });
    await evaluate("location.hash = '#overview'; window.scrollTo(0,0)");
    await settle();
    assert(await evaluate("document.documentElement.scrollWidth <= innerWidth"), `mobile ${width} horizontal overflow`);
    await screenshot(`mobile-${width}-overview`);
    await evaluate(`${action("+ Increment")}.scrollIntoView({block: 'center'})`);
    if (width === 390) {
      await settle();
      const position = await evaluate(`(() => { const area = ${action("+ Increment")}.getBoundingClientRect(); return {x: area.x + area.width/2, y: area.y + area.height/2}; })()`);
      await send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ ...position, radiusX: 1, radiusY: 1, force: 1, id: 1 }] });
      await send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
      await settle();
      assert(await evaluate("document.body.textContent.includes('001')"), "touch activation updates count exactly once");
    }
    await screenshot(`mobile-${width}-model`);
    await evaluate("location.hash = '#comparisons'");
    await settle();
    await screenshot(`mobile-${width}-comparisons`);
  }
  await evaluate("location.hash = '#overview'");
  await settle();
  await evaluate("Object.defineProperty(navigator, 'clipboard', {configurable: true, value: {writeText: async text => {window.copiedSnippet = text}}})");
  await evaluate(`${action("Copy snippet")}.click()`);
  await settle();
  assert(await evaluate("window.copiedSnippet.includes('  fn render(&mut self')"), "copy preserves the code snippet's indentation");
  await evaluate("navigator.clipboard.writeText = async () => {throw new DOMException('Clipboard denied', 'NotAllowedError')}");
  await evaluate(`${action("Copy snippet")}.click()`);
  await settle();
  await evaluate(`${action("Reset")}.click()`);
  await settle();
  assert(await evaluate("document.body.textContent.includes('000')"), "clipboard denial must not freeze the interface");
  await send("Page.navigate", { url: `${base}#comparisons` });
  const reloaded = Date.now();
  while (!(await evaluate("document.querySelector('#status')?.hidden === true && document.title === 'Comparisons — Blit'"))) {
    assert(Date.now() - reloaded < 15000, "comparison deep link failed to load");
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  await settle();
  assert(await evaluate("document.body.textContent.includes('52,689')"), "comparison deep link renders content");
  assert.equal(errors.length, 0, JSON.stringify(errors));
  console.log("PASS: startup, native scroll, pointer + keyboard + touch input, reset, benchmark scales, all comparison tabs, deep link, desktop and 320/390px layouts");
  console.log("Screenshots: target/site-preview/");
} finally {
  socket.close();
  await fetch(`${debuggerUrl}/json/close/${target.id}`);
}
