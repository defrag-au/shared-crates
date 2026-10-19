// macroquad-cnft: HTTP GETs and browser image decoding over the miniquad
// plugin bridge, for games built around Cardano NFTs.
//
// Same shape as wallet.js and discord.js: every call returns an integer id at
// once, the result lands in a map, and Rust polls the id each frame until it
// settles. `cnft_poll` consumes a settled entry.
//
// String arguments arrive as INTEGER HANDLES to sapp_jsutils objects. Read
// them with `consume_js_object(handle)`, which returns the string and frees
// the slot. Calling `.toString()` on a handle is a TypeError that surfaces in
// Rust as `unreachable`.
//
// Why not quad-net: its HTTP reply carries no status. A 404 and a 200 arrive
// as the same "here is some text", so a game cannot tell "this player holds
// nothing" from "the service is down". Every failure here says which it was.

(function () {
    // id -> { status: 'pending' } | { status: 'ok', data } | { status: 'err', data }
    const pending = new Map();
    let nextId = 1;

    // Decoded pixels waiting to be collected: id -> Uint8Array (RGBA).
    // Separate from `pending`, which carries strings; these are megabytes and
    // cross as a byte buffer.
    const decoded = new Map();

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
    function cnft_fetch(url_js, bearer_js) {
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
    // and GIF natively for nothing, so the IIIF service can send whatever is
    // smallest (`default.auto`) and Rust receives raw RGBA either way.
    function cnft_decode_image(url_js) {
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
    function cnft_image_bytes(id) {
        const bytes = decoded.get(id) || new Uint8Array(0);
        decoded.delete(id);
        return js_object(bytes);
    }

    function cnft_poll(id) {
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

    function register_plugin(importObject) {
        importObject.env.cnft_fetch = cnft_fetch;
        importObject.env.cnft_decode_image = cnft_decode_image;
        importObject.env.cnft_image_bytes = cnft_image_bytes;
        importObject.env.cnft_poll = cnft_poll;
    }

    miniquad_add_plugin({ register_plugin, version: 1, name: 'cnft' });
})();
