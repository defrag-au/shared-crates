// Cardano CIP-30 wallet bridge — miniquad plugin exposing the wallet
// surface to Rust. See ../src/bridge.rs for the Rust-side wrapper.
//
// Wraps the standard CIP-30 surface (`window.cardano.<name>.enable()`,
// `getRewardAddresses()`, `getBalance()`, `getUtxos()`,
// `getChangeAddress()`, `signTx()`, `submitTx()`, `signData()`) so the
// Rust side can fire-and-forget requests and poll for completion across
// frames. Promises are stashed in a `pending` Map keyed by an integer
// request id; once a promise resolves or rejects, its entry
// transitions from `pending` to `ok` / `err` and `wallet_poll(id)`
// returns the payload (then drops the entry, since the Rust side has
// consumed it).
//
// Surface: list, connect, network id, reward address, balance, utxos,
// change address, signTx, submitTx, signData, disconnect, cancel, poll, icon.
//
// Plugin version 3: adds `wallet_network_id` and `wallet_cancel`, and gives
// every error a `kind` (`declined` or `failed`) so a host can tell a user's
// refusal from a wallet fault without matching on prose.
//
// Plugin version 4: adds `wallet_icon` and `wallet_icon_pixels` — a wallet's
// announced icon, rasterised here on a canvas so Rust gets plain RGBA and
// never has to decode an SVG.

(function () {
    let nextReqId = 1;
    const pending = new Map();
    // Rasterised icons, keyed by the request that asked for them, until Rust
    // takes the pixels with `wallet_icon_pixels`.
    const icons = new Map();
    let api = null; // the currently-enabled CIP-30 API handle

    function errMsg(e) {
        if (!e) return 'unknown error';
        if (typeof e === 'string') return e;
        // CIP-30 errors carry { code, info }; favour info for human-readable text.
        if (e.info) return e.info;
        if (e.message) return e.message;
        try { return JSON.stringify(e); } catch (_) { return String(e); }
    }

    function jsonObj(value) {
        return js_object(JSON.stringify(value));
    }

    // CIP-30 has no single "declined" code: APIError.Refused is -3, but 2 means
    // UserDeclined for a signature and Failure for a send. Mirror the Rust
    // classifier: -3, or unambiguous decline wording. Deliberately does NOT
    // match a bare "refused" (that is the node refusing a transaction).
    function failure_of(e) {
        const message = errMsg(e);
        const code = (e && typeof e === 'object') ? e.code : undefined;
        const lower = message.toLowerCase();
        const declined = code === -3
            || lower.includes('declin')
            || lower.includes('user cancel')
            || lower.includes('cancelled by')
            || lower.includes('canceled by')
            || lower.includes('rejected by user')
            || lower.includes('user rejected');
        return { kind: declined ? 'declined' : 'failed', message: message };
    }

    function fail(id, message, kind) {
        pending.set(id, { status: 'err', data: message, kind: kind || 'failed' });
    }

    function wallet_list_providers() {
        if (typeof window === 'undefined' || !window.cardano) {
            return jsonObj([]);
        }
        const providers = [];
        for (const key of Object.keys(window.cardano)) {
            const p = window.cardano[key];
            // CIP-30 says a wallet exposes `enable()` directly on
            // window.cardano[key]. Some helper namespaces (e.g.
            // `nightly-connect`) don't, so filter for the real wallets.
            if (p && typeof p.enable === 'function') {
                providers.push({
                    key,
                    name: p.name || key,
                    version: p.apiVersion || ''
                });
            }
        }
        return jsonObj(providers);
    }

    function wallet_connect(name_handle) {
        const name = consume_js_object(name_handle);
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        const provider = window.cardano && window.cardano[name];
        if (!provider) {
            fail(id, `wallet '${name}' not found`);
            return id;
        }
        provider.enable()
            .then(enabled => {
                api = enabled;
                pending.set(id, { status: 'ok', data: name });
            })
            .catch(err => {
                const f = failure_of(err);
                fail(id, f.message, f.kind);
            });
        return id;
    }

    // CIP-30 getNetworkId: 0 = testnet, 1 = mainnet. Fetched during connect so
    // a mainnet-only surface can refuse the wrong wallet before it builds.
    function wallet_network_id() {
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            fail(id, 'no wallet connected');
            return id;
        }
        api.getNetworkId()
            .then(networkId => {
                pending.set(id, { status: 'ok', data: String(networkId) });
            })
            .catch(err => {
                const f = failure_of(err);
                fail(id, f.message, f.kind);
            });
        return id;
    }

    function wallet_reward_address() {
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            fail(id, 'no wallet connected');
            return id;
        }
        api.getRewardAddresses()
            .then(addrs => {
                const first = (addrs && addrs[0]) || '';
                pending.set(id, { status: 'ok', data: first });
            })
            .catch(err => {
                const f = failure_of(err);
                fail(id, f.message, f.kind);
            });
        return id;
    }

    function wallet_balance() {
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            fail(id, 'no wallet connected');
            return id;
        }
        // Returns CBOR-encoded Value (hex). Either a uint (just lovelace)
        // or [lovelace, multiasset] where multiasset is a nested map
        // {policy_id: {asset_name: quantity}}. The Rust side walks this
        // via minicbor to extract the ADA Handle if present.
        api.getBalance()
            .then(cbor => {
                pending.set(id, { status: 'ok', data: String(cbor || '') });
            })
            .catch(err => {
                const f = failure_of(err);
                fail(id, f.message, f.kind);
            });
        return id;
    }

    function wallet_utxos() {
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            fail(id, 'no wallet connected');
            return id;
        }
        // CIP-30 getUtxos() → array of hex-encoded CBOR
        // TransactionUnspentOutput, or null when the wallet holds none.
        // JSON-encode so the whole list crosses the single poll `data`
        // string; the Rust side parses via wallet_miniquad::parse_utxos.
        api.getUtxos()
            .then(utxos => {
                pending.set(id, { status: 'ok', data: JSON.stringify(utxos || []) });
            })
            .catch(err => {
                const f = failure_of(err);
                fail(id, f.message, f.kind);
            });
        return id;
    }

    function wallet_change_address() {
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            fail(id, 'no wallet connected');
            return id;
        }
        api.getChangeAddress()
            .then(addr => {
                pending.set(id, { status: 'ok', data: String(addr || '') });
            })
            .catch(err => {
                const f = failure_of(err);
                fail(id, f.message, f.kind);
            });
        return id;
    }

    function wallet_sign_tx(tx_handle, partial_sign) {
        const txHex = consume_js_object(tx_handle);
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            fail(id, 'no wallet connected');
            return id;
        }
        // CIP-30 signTx(txHex, partialSign) → witness-set hex. partial_sign
        // arrives as a number (0/1) over the miniquad FFI boundary.
        api.signTx(txHex, partial_sign !== 0)
            .then(witness => {
                pending.set(id, { status: 'ok', data: String(witness || '') });
            })
            .catch(err => {
                const f = failure_of(err);
                fail(id, f.message, f.kind);
            });
        return id;
    }

    function wallet_submit_tx(tx_handle) {
        const txHex = consume_js_object(tx_handle);
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            fail(id, 'no wallet connected');
            return id;
        }
        // CIP-30 submitTx(txHex) → tx hash.
        api.submitTx(txHex)
            .then(hash => {
                pending.set(id, { status: 'ok', data: String(hash || '') });
            })
            .catch(err => {
                const f = failure_of(err);
                fail(id, f.message, f.kind);
            });
        return id;
    }

    function wallet_disconnect() {
        api = null;
    }

    function wallet_sign_data(addr_handle, payload_handle) {
        const addr = consume_js_object(addr_handle);
        const payload = consume_js_object(payload_handle);
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            fail(id, 'no wallet connected');
            return id;
        }
        // CIP-30 signData expects hex-encoded payload bytes; the address
        // can be a payment or stake address in hex. Returns
        // { signature, key } where signature is COSE_Sign1 hex and key
        // is COSE_Key hex (the public key in CBOR form).
        api.signData(addr, payload)
            .then(sig => {
                pending.set(id, { status: 'ok', data: JSON.stringify(sig) });
            })
            .catch(err => {
                const f = failure_of(err);
                fail(id, f.message, f.kind);
            });
        return id;
    }

    // CIP-30 `icon`: a URI the wallet announces on window.cardano[key] — a
    // data URI in practice, SVG as often as PNG. The browser decodes it; it is
    // drawn centred and contained on a `size`×`size` canvas and read back as
    // RGBA, so the Rust side gets pixels whatever the format. On ok, `data` is
    // the size; the pixels wait in `icons` for `wallet_icon_pixels`.
    function wallet_icon(key_handle, size) {
        const key = consume_js_object(key_handle);
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        const provider = window.cardano && window.cardano[key];
        const src = provider && provider.icon;
        if (typeof src !== 'string' || src.length === 0) {
            fail(id, `wallet '${key}' announces no icon`);
            return id;
        }
        const img = new Image();
        // A remote icon can only be read back if its server allows it; a data
        // URI never needs this.
        if (/^https?:/i.test(src)) img.crossOrigin = 'anonymous';
        img.onload = () => {
            if (!pending.has(id)) return; // cancelled while loading
            try {
                const canvas = document.createElement('canvas');
                canvas.width = size;
                canvas.height = size;
                const ctx = canvas.getContext('2d');
                // An SVG with no intrinsic size reports 0; fill the square.
                const w = img.naturalWidth || size;
                const h = img.naturalHeight || size;
                const scale = Math.min(size / w, size / h);
                const dw = w * scale;
                const dh = h * scale;
                ctx.drawImage(img, (size - dw) / 2, (size - dh) / 2, dw, dh);
                const rgba = ctx.getImageData(0, 0, size, size).data;
                icons.set(id, new Uint8Array(rgba.buffer.slice(0)));
                pending.set(id, { status: 'ok', data: String(size) });
            } catch (e) {
                // A tainted canvas (a remote icon without CORS) lands here.
                fail(id, `wallet '${key}' icon could not be read: ${errMsg(e)}`);
            }
        };
        img.onerror = () => {
            if (pending.has(id)) fail(id, `wallet '${key}' icon did not load`);
        };
        img.src = src;
        return id;
    }

    // The pixels a resolved `wallet_icon` produced, once: taken, then dropped.
    // An empty buffer for an id with none.
    function wallet_icon_pixels(req_id) {
        const px = icons.get(req_id);
        icons.delete(req_id);
        return js_object(px || new Uint8Array(0));
    }

    // Abandon a request. The JS side drops the entry; a promise that resolves
    // later has nowhere to land. Used when a host times out a read so the map
    // cannot grow without bound.
    function wallet_cancel(req_id) {
        pending.delete(req_id);
        icons.delete(req_id);
    }

    function wallet_poll(req_id) {
        const r = pending.get(req_id);
        if (!r) {
            return jsonObj({ status: 'err', data: `unknown req ${req_id}`, kind: 'failed' });
        }
        if (r.status === 'pending') {
            return jsonObj({ status: 'pending' });
        }
        // Rust has consumed this result — drop it so the map doesn't grow.
        pending.delete(req_id);
        const out = { status: r.status, data: String(r.data) };
        if (r.status === 'err') out.kind = r.kind || 'failed';
        return jsonObj(out);
    }

    register_plugin = function (importObject) {
        importObject.env.wallet_list_providers = wallet_list_providers;
        importObject.env.wallet_connect = wallet_connect;
        importObject.env.wallet_network_id = wallet_network_id;
        importObject.env.wallet_reward_address = wallet_reward_address;
        importObject.env.wallet_balance = wallet_balance;
        importObject.env.wallet_utxos = wallet_utxos;
        importObject.env.wallet_change_address = wallet_change_address;
        importObject.env.wallet_sign_tx = wallet_sign_tx;
        importObject.env.wallet_submit_tx = wallet_submit_tx;
        importObject.env.wallet_disconnect = wallet_disconnect;
        importObject.env.wallet_sign_data = wallet_sign_data;
        importObject.env.wallet_cancel = wallet_cancel;
        importObject.env.wallet_poll = wallet_poll;
        importObject.env.wallet_icon = wallet_icon;
        importObject.env.wallet_icon_pixels = wallet_icon_pixels;
    };

    miniquad_add_plugin({ register_plugin, version: 4, name: 'wallet' });
})();
