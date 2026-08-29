// Screenshot at a TRUE narrow viewport — the storybook, or a deployed app.
//
// `--window-size=390,…` cannot do this: Chromium headless floors the window
// near 620px, lays out at the floor and then CROPS the capture to the width you
// asked for. A "390px" shot is therefore a wide layout with its right edge
// sliced off — which reads as an overflow bug that isn't there, and hides the
// real one. CDP's Emulation.setDeviceMetricsOverride sets the real layout
// viewport instead.
//
//   node cdp-shot.mjs <url> <width> <height> <out.png> [settleMs]
//
// Pair it with `?nav=0` on the storybook, which drops the 180px sidebar so the
// story gets the whole viewport rather than `viewport − 180`.
//
// Requires Brave already running headless with --remote-debugging-port=9222 AND
// a --user-data-dir (Brave refuses to expose the port without one):
//
//   BRAVE="/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"
//   "$BRAVE" --headless=new --disable-gpu --use-gl=swiftshader \
//     --enable-unsafe-swiftshader --hide-scrollbars \
//     --remote-debugging-port=9222 --user-data-dir=/tmp/brave-cdp-profile \
//     about:blank &
//
// Node has a global WebSocket, so there are no dependencies to install.
import { writeFileSync } from 'node:fs';

const [url, w, h, out, wait] = process.argv.slice(2);
if (!url || !w || !h || !out) {
    console.error('usage: cdp-shot.mjs <url> <width> <height> <out.png> [settleMs]');
    process.exit(2);
}
const width = Number(w);
const height = Number(h);
// wasm boot + remote font fetch. A live app also needs its socket and first
// payload, which is a lot longer than the storybook's static data.
const settleMs = Number(wait ?? 9000);

const targets = await (await fetch('http://127.0.0.1:9222/json')).json();
const page = targets.find((t) => t.type === 'page');
if (!page) throw new Error('no page target — is Brave up with --remote-debugging-port?');

const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();
const send = (method, params = {}) =>
    new Promise((resolve) => {
        const n = ++id;
        pending.set(n, resolve);
        ws.send(JSON.stringify({ id: n, method, params }));
    });

ws.addEventListener('message', (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
        pending.get(msg.id)(msg.result);
        pending.delete(msg.id);
    }
});

await new Promise((r) => ws.addEventListener('open', r));
await send('Page.enable');
// mobile:true so the page gets the same viewport semantics a phone gives it.
await send('Emulation.setDeviceMetricsOverride', {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: true,
});
await send('Page.navigate', { url });
await new Promise((r) => setTimeout(r, settleMs));
const shot = await send('Page.captureScreenshot', { format: 'png' });
writeFileSync(out, Buffer.from(shot.data, 'base64'));
console.log(`${out} @ ${width}x${height}`);
ws.close();
