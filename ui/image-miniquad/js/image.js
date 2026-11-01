// image-miniquad: HTTP GETs and browser image decoding over the miniquad
// plugin bridge, for any macroquad game.
//
// Same shape as wallet.js and discord.js: every call returns an integer id at
// once, the result lands in a map, and Rust polls the id each frame until it
// settles. `image_poll` consumes a settled entry.
//
// Three kinds of request share that machinery:
//   image_fetch        → text
//   image_decode       → RGBA, decoded by the browser
//   image_fetch_bytes  → raw bytes and the response's status
//
// String arguments arrive as INTEGER HANDLES to sapp_jsutils objects. Read
// them with `consume_js_object(handle)`, which returns the string and frees the
// slot. Calling `.toString()` on a handle is a TypeError that surfaces in
// Rust as `unreachable`.
//
// Why not quad-net: its HTTP reply carries no status. A 404 and a 200 arrive
// as the same "here is some text", so a game cannot tell "there is nothing
// here" from "the service is down". Every failure here says which it was.

(function () {
    // id -> { status: 'pending' } | { status: 'ok', data } | { status: 'err', data }
    const pending = new Map();
    let nextId = 1;

    // Decoded pixels waiting to be collected: id -> Uint8Array (RGBA).
    // Separate from `pending`, which carries strings; these are megabytes and
    // cross as a byte buffer.
    const decoded = new Map();

    // Raw response bodies waiting to be collected: id -> Uint8Array.
    // Deliberately not `decoded`: the two are different promises, and a caller
    // that mixed them would get pixels where it expected a tile.
    const bodies = new Map();

    function newId() {
        const id = nextId++;
        pending.set(id, { status: 'pending' });
        return id;
    }

    function settle(id, status, data) {
        if (pending.has(id)) pending.set(id, { status, data });
    }

    function errMsg(e) {
        return e && e.message ? e.message : String(e);
    }

    // Failures are JSON, so Rust can decide whether to retry without parsing
    // prose. `stage` is where it failed: 'network' (no response at all),
    // 'http' (a response that was not 2xx) or 'decode' (bytes that are not an
    // image this browser reads).
    function fail(id, stage, status, message) {
        settle(id, 'err', JSON.stringify({ stage, status, message }));
    }

    // GET, response as text. `bearer` may be empty for a public route.
    function image_fetch(url_js, bearer_js) {
        const id = newId();
        const url = consume_js_object(url_js);
        const bearer = consume_js_object(bearer_js);
        const headers = bearer ? { Authorization: `Bearer ${bearer}` } : {};
        fetch(url, { method: 'GET', headers })
            .then(async (r) => {
                const text = await r.text();
                if (r.ok) settle(id, 'ok', text);
                else fail(id, 'http', r.status, text.slice(0, 500));
            })
            .catch((e) => fail(id, 'network', 0, errMsg(e)));
        return id;
    }

    // Fetch and decode an image with the BROWSER's decoders.
    //
    // macroquad decodes PNG and TGA only, and a JPEG decoder in wasm cost
    // ~235 KB in discord-miniquad. The browser decodes JPEG, PNG, WebP, AVIF
    // and GIF natively for nothing, so a service can send whatever is
    // smallest (`default.auto`) and Rust receives raw RGBA either way.
    function image_decode(url_js) {
        const id = newId();
        const url = consume_js_object(url_js);
        (async () => {
            let res;
            try {
                res = await fetch(url);
            } catch (e) {
                fail(id, 'network', 0, errMsg(e));
                return;
            }
            if (!res.ok) {
                fail(id, 'http', res.status, res.statusText);
                return;
            }
            try {
                const bitmap = await createImageBitmap(await res.blob());
                const { width, height } = bitmap;
                const canvas = new OffscreenCanvas(width, height);
                const ctx = canvas.getContext('2d');
                ctx.drawImage(bitmap, 0, 0);
                // Free the decoded bitmap now rather than at GC time; a
                // roster of these is a lot of retained memory otherwise.
                bitmap.close();
                const pixels = ctx.getImageData(0, 0, width, height).data;
                decoded.set(id, new Uint8Array(pixels.buffer));
                settle(id, 'ok', JSON.stringify({ width, height }));
            } catch (e) {
                fail(id, 'decode', res.status, errMsg(e));
            }
        })();
        return id;
    }

    // Hand over a completed decode's pixels, exactly once. An unknown or
    // already-collected id gives an empty buffer, which Rust reads as "no
    // image" rather than hanging.
    function image_bytes(id) {
        const bytes = decoded.get(id) || new Uint8Array(0);
        decoded.delete(id);
        return js_object(bytes);
    }

    function image_poll(id) {
        const entry = pending.get(id);
        if (!entry) {
            return js_object(
                JSON.stringify({
                    status: 'err',
                    data: JSON.stringify({ stage: 'network', status: 0, message: 'unknown request' }),
                }),
            );
        }
        if (entry.status === 'pending') return js_object('{"status":"pending"}');
        pending.delete(id);
        return js_object(JSON.stringify(entry));
    }

    // The archive/object length behind a response: `Content-Range:
    // bytes 0-126/123456` names 123456, and a plain 200 has no such header, so
    // the body length is the whole thing.
    //
    // A 206 without `Content-Range` is a server that did not answer the
    // question; the body length is the best available answer and Rust is told
    // the status so it can refuse.
    function totalOf(contentRange, length) {
        if (!contentRange) return length;
        const slash = contentRange.lastIndexOf('/');
        if (slash < 0) return length;
        const total = Number(contentRange.slice(slash + 1));
        return Number.isFinite(total) ? total : length;
    }

    // GET, response as raw bytes. `range_js` is a `Range` header value
    // (`"bytes=0-126"`) or the empty string; `bearer_js` may be empty for a
    // public route.
    //
    // The status comes back with the bytes, because a range request has two
    // honest answers: 206 with the slice asked for, or 200 with the whole object
    // because the server ignored the header. Slicing a 200 as if it were a 206
    // reads the wrong bytes silently.
    //
    // Reading `Content-Range` from a cross-origin response needs the server to
    // send `Access-Control-Expose-Headers: Content-Range`.
    function image_fetch_bytes(url_js, range_js, bearer_js) {
        const id = newId();
        const url = consume_js_object(url_js);
        const range = consume_js_object(range_js);
        const bearer = consume_js_object(bearer_js);
        const headers = {};
        if (range) headers.Range = range;
        if (bearer) headers.Authorization = `Bearer ${bearer}`;
        (async () => {
            let r;
            try {
                r = await fetch(url, { method: 'GET', headers });
            } catch (e) {
                fail(id, 'network', 0, errMsg(e));
                return;
            }
            if (!r.ok) {
                fail(id, 'http', r.status, r.statusText);
                return;
            }
            let bytes;
            try {
                bytes = new Uint8Array(await r.arrayBuffer());
            } catch (e) {
                fail(id, 'network', r.status, errMsg(e));
                return;
            }
            bodies.set(id, bytes);
            settle(
                id,
                'ok',
                JSON.stringify({
                    status: r.status,
                    length: bytes.length,
                    total: totalOf(r.headers.get('content-range'), bytes.length),
                }),
            );
        })();
        return id;
    }

    // Hand over a completed body, exactly once. An unknown or already-collected
    // id gives an empty buffer, which Rust reads as a short body rather than
    // hanging.
    function image_body(id) {
        const bytes = bodies.get(id) || new Uint8Array(0);
        bodies.delete(id);
        return js_object(bytes);
    }

    function register_plugin(importObject) {
        importObject.env.image_fetch = image_fetch;
        importObject.env.image_decode = image_decode;
        importObject.env.image_bytes = image_bytes;
        importObject.env.image_fetch_bytes = image_fetch_bytes;
        importObject.env.image_body = image_body;
        importObject.env.image_poll = image_poll;
    }

    miniquad_add_plugin({ register_plugin, version: 2, name: 'image' });
})();
