// Discord Embedded App SDK protocol — spoken directly, over the miniquad
// plugin bridge.
//
// The official SDK is a TypeScript wrapper around a plain `postMessage` RPC
// with the Discord client, and that protocol is small enough to speak in Rust:
//
//   source     = window.parent.opener ?? window.parent
//   origin     = document.referrer || '*'
//   handshake  = postMessage([0, {v:1, encoding:'json', client_id, frame_id}])
//   command    = postMessage([1, {cmd, args, nonce}])   -> reply carries nonce
//   opcodes    = HANDSHAKE 0 · FRAME 1 · CLOSE 2 · HELLO 3
//
// String arguments arrive from Rust as INTEGER HANDLES to sapp_jsutils
// objects — read them with `consume_js_object(handle)`, which returns the
// string and frees the slot. Not `.to_string()`; a handle is a number, and
// the wrong call is a TypeError that surfaces in Rust as `unreachable`.
//
// So this file is transport only. It owns no Discord semantics beyond the
// handshake — command names, argument shapes and response parsing all live in
// Rust, which keeps the surface identical to `wallet-miniquad`: fire a request,
// get an integer id, poll across frames.
//
// Bundling the real SDK instead would mean shipping a JS dependency whose
// types Rust can't see, to wrap a protocol Rust can already build. The nonce
// even does the correlation for us — `wallet.js` had to invent that.

(function () {
    const OP_HANDSHAKE = 0;
    const OP_FRAME = 1;
    const OP_CLOSE = 2;

    // Same shape as wallet.js: an integer id per in-flight request, entries
    // transitioning pending -> ok/err, consumed by poll().
    const pending = new Map();
    let nextId = 1;

    // nonce -> req id, so a FRAME reply can be routed back to its caller.
    const byNonce = new Map();

    let source = null;
    let origin = '*';
    let ready = false;
    // Resolved once READY arrives; connect() hands its id here.
    let readyReq = null;

    function errMsg(e) {
        return e && e.message ? e.message : String(e);
    }

    function newId() {
        const id = nextId++;
        pending.set(id, { status: 'pending' });
        return id;
    }

    function settle(id, status, data) {
        if (id != null && pending.has(id)) {
            pending.set(id, { status, data });
        }
    }

    // RFC4122-ish v4. `crypto.randomUUID` is not available in every webview
    // Discord embeds, and the nonce only needs to be unique per frame.
    function uuid() {
        const b = new Uint8Array(16);
        crypto.getRandomValues(b);
        b[6] = (b[6] & 0x0f) | 0x40;
        b[8] = (b[8] & 0x3f) | 0x80;
        const h = [...b].map((x) => x.toString(16).padStart(2, '0'));
        return `${h.slice(0, 4).join('')}-${h.slice(4, 6).join('')}-${h
            .slice(6, 8)
            .join('')}-${h.slice(8, 10).join('')}-${h.slice(10).join('')}`;
    }

    // Everything Discord hands the iframe at launch. `custom_id` is the
    // passthrough slot: whatever the interaction attached when it responded
    // with LAUNCH_ACTIVITY arrives here, which is how an Activity knows which
    // mission run (or squad, or match) it was opened for.
    function discord_launch_context() {
        const p = new URLSearchParams(window.location.search);
        const ctx = {};
        for (const k of [
            'frame_id',
            'instance_id',
            'platform',
            'custom_id',
            'referrer_id',
            'guild_id',
            'channel_id',
            'location_id',
            'mobile_app_version',
        ]) {
            const v = p.get(k);
            if (v !== null) ctx[k] = v;
        }
        return js_object(JSON.stringify(ctx));
    }

    // One query parameter by name, or the empty string. For anything the page
    // wants to pass the app that isn't part of Discord's launch context —
    // a client id, a debug flag. Empty means absent; a real empty value is
    // indistinguishable, which is fine for flags and ids.
    function discord_launch_query(key_js) {
        const v = new URLSearchParams(window.location.search).get(consume_js_object(key_js));
        return js_object(v === null ? '' : v);
    }

    function discord_connect(client_id_js) {
        const clientId = consume_js_object(client_id_js);
        const id = newId();

        const params = new URLSearchParams(window.location.search);
        const frameId = params.get('frame_id');
        if (!frameId) {
            // Not running inside Discord. Say so plainly rather than hanging
            // on a handshake that will never be answered.
            settle(id, 'err', 'no frame_id — not launched as an Activity');
            return id;
        }

        // A popout Activity is a child of the popout window, whose opener is
        // the main client where the RPC server actually lives.
        source = window.parent.opener ?? window.parent;
        origin = document.referrer || '*';
        readyReq = id;

        window.addEventListener('message', onMessage);

        try {
            source.postMessage(
                [OP_HANDSHAKE, { v: 1, encoding: 'json', client_id: clientId, frame_id: frameId }],
                origin,
            );
        } catch (e) {
            settle(id, 'err', errMsg(e));
        }
        return id;
    }

    function onMessage(event) {
        const data = event.data;
        if (!Array.isArray(data) || data.length < 2) return;
        const [opcode, payload] = data;

        if (opcode === OP_CLOSE) {
            ready = false;
            // Fail everything still waiting; a closed frame will never reply.
            for (const [id, entry] of pending) {
                if (entry.status === 'pending') {
                    settle(id, 'err', `frame closed: ${payload && payload.message}`);
                }
            }
            return;
        }

        if (opcode !== OP_FRAME || !payload) return;

        // READY is an event, not a command reply — it carries no nonce, so it
        // resolves the connect() request specifically.
        if (payload.evt === 'READY') {
            ready = true;
            if (readyReq != null) {
                settle(readyReq, 'ok', JSON.stringify(payload.data ?? {}));
                readyReq = null;
            }
            return;
        }

        const id = payload.nonce != null ? byNonce.get(payload.nonce) : undefined;
        if (id === undefined) return;
        byNonce.delete(payload.nonce);

        if (payload.evt === 'ERROR') {
            settle(id, 'err', JSON.stringify(payload.data ?? {}));
        } else {
            settle(id, 'ok', JSON.stringify(payload.data ?? {}));
        }
    }

    // Generic command dispatch. The SDK wraps 29 of these; there is no reason
    // to enumerate them twice when the wire shape is identical and Rust can
    // build the args with serde.
    function discord_command(cmd_js, args_json_js) {
        const id = newId();
        if (!ready) {
            settle(id, 'err', 'not connected — call connect() and await READY first');
            return id;
        }

        const cmd = consume_js_object(cmd_js);
        let args;
        try {
            args = JSON.parse(consume_js_object(args_json_js));
        } catch (e) {
            settle(id, 'err', `args are not valid JSON: ${errMsg(e)}`);
            return id;
        }

        const nonce = uuid();
        byNonce.set(nonce, id);
        try {
            source.postMessage([OP_FRAME, { cmd, args, nonce }], origin);
        } catch (e) {
            byNonce.delete(nonce);
            settle(id, 'err', errMsg(e));
        }
        return id;
    }

    // Same-origin POST, for the code-for-token exchange. Relative URLs resolve
    // under the Activity's own root, which Discord's proxy maps — so a caller
    // passes "api/token" and it lands on the mapped worker as
    // <root>/api/token. Kept minimal (JSON in, text out) because it exists for
    // exactly one call; a game that needs a real HTTP client should get one.
    // `bearer` may be the empty string — the token exchange runs before there
    // is any token to send, so an unauthenticated POST is the normal first
    // call, not a degenerate case.
    function discord_http_post(url_js, body_js, bearer_js) {
        const id = newId();
        const url = consume_js_object(url_js);
        const body = consume_js_object(body_js);
        const bearer = consume_js_object(bearer_js);
        const headers = { 'Content-Type': 'application/json' };
        if (bearer) headers.Authorization = `Bearer ${bearer}`;
        fetch(url, {
            method: 'POST',
            headers,
            body,
        })
            .then(async (r) => {
                const text = await r.text();
                if (r.ok) settle(id, 'ok', text);
                else settle(id, 'err', `${r.status}: ${text}`);
            })
            .catch((e) => settle(id, 'err', errMsg(e)));
        return id;
    }

    // Decoded image pixels waiting to be collected: id -> Uint8Array(RGBA).
    // Separate from `pending` because that map carries strings; these are
    // megabytes of pixels and are handed over as a byte buffer instead.
    const decoded = new Map();

    // Fetch and decode an image using the BROWSER's decoders.
    //
    // Every wasm runtime that decodes images in-process pays for it twice: in
    // bundle size, and in formats it can't read. macroquad ships `image` with
    // png+tga only, and adding jpeg cost ~235 KB of wasm. WebP would be worse
    // — the Rust decoders are patchy and libwebp in wasm is genuinely painful.
    //
    // The browser already has native decoders for JPEG, PNG, WebP, AVIF and
    // GIF, written in C++, often GPU-assisted, costing zero bundle bytes. So
    // decode there and hand Rust the raw RGBA: `createImageBitmap` (which
    // decodes off the main thread) into an `OffscreenCanvas`, then
    // `getImageData`.
    //
    // Mirrors the approach `egui-widgets`' image_loader already uses on the
    // wasm-bindgen side; this is the same trick expressed through the miniquad
    // plugin protocol, since wasm-bindgen can never run under miniquad.
    function discord_decode_image(url_js) {
        const id = newId();
        const url = consume_js_object(url_js);
        (async () => {
            try {
                const res = await fetch(url);
                if (!res.ok) {
                    settle(id, 'err', `${res.status}: ${res.statusText}`);
                    return;
                }
                const bitmap = await createImageBitmap(await res.blob());
                const { width, height } = bitmap;
                const canvas = new OffscreenCanvas(width, height);
                const ctx = canvas.getContext('2d', { willReadFrequently: false });
                ctx.drawImage(bitmap, 0, 0);
                // Free the decoded bitmap now rather than at GC time; a roster
                // of these is a lot of retained memory otherwise.
                bitmap.close();
                const pixels = ctx.getImageData(0, 0, width, height).data;
                decoded.set(id, new Uint8Array(pixels.buffer));
                settle(id, 'ok', JSON.stringify({ width, height }));
            } catch (e) {
                settle(id, 'err', errMsg(e));
            }
        })();
        return id;
    }

    // Hand over the pixels for a completed decode, exactly once.
    //
    // Returns an empty buffer if the id is unknown or already collected, which
    // Rust reads as "no image" rather than as a hang.
    function discord_image_bytes(id) {
        const bytes = decoded.get(id) || new Uint8Array(0);
        decoded.delete(id);
        return js_object(bytes);
    }

    // Ask Discord to dismiss the Activity.
    //
    // Fire-and-forget: CLOSE is the one frame the client does not answer, so
    // there is no request id to poll. After this the iframe is going away, and
    // anything still in flight will never settle.
    function discord_close(code, message_js) {
        const message = consume_js_object(message_js);
        if (!source) return;
        source.postMessage([OP_CLOSE, { code, message }], origin);
    }

    // Same-origin GET with a bearer token — the widget token the exchange
    // minted, so the Activity can call the platform's `/api/*` routes as the
    // widget it now is. Same minimal shape as the POST.
    function discord_http_get(url_js, bearer_js) {
        const id = newId();
        const url = consume_js_object(url_js);
        const bearer = consume_js_object(bearer_js);
        const headers = bearer ? { Authorization: `Bearer ${bearer}` } : {};
        fetch(url, { method: 'GET', headers })
            .then(async (r) => {
                const text = await r.text();
                if (r.ok) settle(id, 'ok', text);
                else settle(id, 'err', `${r.status}: ${text}`);
            })
            .catch((e) => settle(id, 'err', errMsg(e)));
        return id;
    }

    function discord_poll(id) {
        const entry = pending.get(id);
        if (!entry) return js_object(JSON.stringify({ status: 'err', data: 'unknown request' }));
        if (entry.status === 'pending') return js_object(JSON.stringify({ status: 'pending' }));
        pending.delete(id);
        return js_object(JSON.stringify(entry));
    }

    function register_plugin(importObject) {
        importObject.env.discord_launch_context = discord_launch_context;
        importObject.env.discord_launch_query = discord_launch_query;
        importObject.env.discord_connect = discord_connect;
        importObject.env.discord_command = discord_command;
        importObject.env.discord_http_post = discord_http_post;
        importObject.env.discord_http_get = discord_http_get;
        importObject.env.discord_close = discord_close;
        importObject.env.discord_decode_image = discord_decode_image;
        importObject.env.discord_image_bytes = discord_image_bytes;
        importObject.env.discord_poll = discord_poll;
    }

    miniquad_add_plugin({ register_plugin, version: 1, name: 'discord' });
})();
