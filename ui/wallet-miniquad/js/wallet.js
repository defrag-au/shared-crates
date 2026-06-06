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
// Surface: list, connect, reward address, balance, utxos, change
// address, signTx, submitTx, signData, disconnect, poll.

(function () {
    let nextReqId = 1;
    const pending = new Map();
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
            pending.set(id, { status: 'err', data: `wallet '${name}' not found` });
            return id;
        }
        provider.enable()
            .then(enabled => {
                api = enabled;
                pending.set(id, { status: 'ok', data: name });
            })
            .catch(err => {
                pending.set(id, { status: 'err', data: errMsg(err) });
            });
        return id;
    }

    function wallet_reward_address() {
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            pending.set(id, { status: 'err', data: 'no wallet connected' });
            return id;
        }
        api.getRewardAddresses()
            .then(addrs => {
                const first = (addrs && addrs[0]) || '';
                pending.set(id, { status: 'ok', data: first });
            })
            .catch(err => {
                pending.set(id, { status: 'err', data: errMsg(err) });
            });
        return id;
    }

    function wallet_balance() {
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            pending.set(id, { status: 'err', data: 'no wallet connected' });
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
                pending.set(id, { status: 'err', data: errMsg(err) });
            });
        return id;
    }

    function wallet_utxos() {
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            pending.set(id, { status: 'err', data: 'no wallet connected' });
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
                pending.set(id, { status: 'err', data: errMsg(err) });
            });
        return id;
    }

    function wallet_change_address() {
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            pending.set(id, { status: 'err', data: 'no wallet connected' });
            return id;
        }
        api.getChangeAddress()
            .then(addr => {
                pending.set(id, { status: 'ok', data: String(addr || '') });
            })
            .catch(err => {
                pending.set(id, { status: 'err', data: errMsg(err) });
            });
        return id;
    }

    function wallet_sign_tx(tx_handle, partial_sign) {
        const txHex = consume_js_object(tx_handle);
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            pending.set(id, { status: 'err', data: 'no wallet connected' });
            return id;
        }
        // CIP-30 signTx(txHex, partialSign) → witness-set hex. partial_sign
        // arrives as a number (0/1) over the miniquad FFI boundary.
        api.signTx(txHex, partial_sign !== 0)
            .then(witness => {
                pending.set(id, { status: 'ok', data: String(witness || '') });
            })
            .catch(err => {
                pending.set(id, { status: 'err', data: errMsg(err) });
            });
        return id;
    }

    function wallet_submit_tx(tx_handle) {
        const txHex = consume_js_object(tx_handle);
        const id = nextReqId++;
        pending.set(id, { status: 'pending' });
        if (!api) {
            pending.set(id, { status: 'err', data: 'no wallet connected' });
            return id;
        }
        // CIP-30 submitTx(txHex) → tx hash.
        api.submitTx(txHex)
            .then(hash => {
                pending.set(id, { status: 'ok', data: String(hash || '') });
            })
            .catch(err => {
                pending.set(id, { status: 'err', data: errMsg(err) });
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
            pending.set(id, { status: 'err', data: 'no wallet connected' });
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
                pending.set(id, { status: 'err', data: errMsg(err) });
            });
        return id;
    }

    function wallet_poll(req_id) {
        const r = pending.get(req_id);
        if (!r) {
            return jsonObj({ status: 'err', data: `unknown req ${req_id}` });
        }
        if (r.status === 'pending') {
            return jsonObj({ status: 'pending' });
        }
        // Rust has consumed this result — drop it so the map doesn't grow.
        pending.delete(req_id);
        return jsonObj({ status: r.status, data: String(r.data) });
    }

    register_plugin = function (importObject) {
        importObject.env.wallet_list_providers = wallet_list_providers;
        importObject.env.wallet_connect = wallet_connect;
        importObject.env.wallet_reward_address = wallet_reward_address;
        importObject.env.wallet_balance = wallet_balance;
        importObject.env.wallet_utxos = wallet_utxos;
        importObject.env.wallet_change_address = wallet_change_address;
        importObject.env.wallet_sign_tx = wallet_sign_tx;
        importObject.env.wallet_submit_tx = wallet_submit_tx;
        importObject.env.wallet_disconnect = wallet_disconnect;
        importObject.env.wallet_sign_data = wallet_sign_data;
        importObject.env.wallet_poll = wallet_poll;
    };

    miniquad_add_plugin({ register_plugin, version: 2, name: 'wallet' });
})();
