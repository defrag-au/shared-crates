// The host side of the offthread ABI: instantiate the module, carry bytes.
//
// This is the whole of what wasm-bindgen would otherwise be doing for us, and
// the size of it is the argument for not using wasm-bindgen here. Four imports
// and a message pump, because a service that does arithmetic needs to receive
// bytes, send bytes, log, and seed a CSPRNG — and nothing else.
//
// Written next to the artifact by the build script, from `offthread::RUNTIME_JS`,
// so it cannot drift from the Rust ABI it implements.
//
// `__OFFTHREAD_WASM__` is substituted with the (content-hashed) module URL.

let wasm = null;

// ⚠️ ALWAYS derive a fresh view. Every call into wasm may grow the memory,
// which DETACHES any ArrayBuffer previously handed out — a view cached across
// a call reads zeroes or throws. This is the single most common way a
// hand-written wasm bridge corrupts data, and it fails intermittently, only
// under load, only once the allocator happens to need another page.
function bytes(ptr, len) {
  return new Uint8Array(wasm.memory.buffer, ptr, len);
}

const imports = {
  offthread: {
    // Copied BEFORE returning, because the caller keeps ownership and frees
    // the buffer as soon as this returns. `slice()` is the copy.
    offthread_post(ptr, len) {
      const message = bytes(ptr, len).slice();
      self.postMessage(message, [message.buffer]);
    },

    // Levels are `log::Level`: 1=Error 2=Warn 3=Info 4=Debug 5=Trace. Mapped
    // onto the console's own severities so a worker's output is filterable in
    // devtools the same way the page's is.
    offthread_log(level, ptr, len) {
      const text = new TextDecoder().decode(bytes(ptr, len));
      const write =
        level === 1
          ? console.error
          : level === 2
            ? console.warn
            : level === 4 || level === 5
              ? console.debug
              : console.info;
      write.call(console, text);
    },

    // `getrandom`'s custom backend lands here. Same CSPRNG the wasm-bindgen
    // path would reach, seeded from OS entropy; only the ABI differs.
    offthread_random(ptr, len) {
      crypto.getRandomValues(bytes(ptr, len));
    },
  },
};

// Requests that arrive before instantiation finishes. Unlike the page side —
// which waits for an explicit `Ready` — this one genuinely cannot answer yet,
// and dropping would strand a caller that did nothing wrong.
const pending = [];
let started = false;

self.onmessage = (event) => {
  if (!started) {
    pending.push(event.data);
    return;
  }
  dispatch(event.data);
};

function dispatch(data) {
  const request = new Uint8Array(data);
  const ptr = wasm.offthread_alloc(request.length);
  // Fresh view AFTER the alloc — see above; alloc is exactly the call that
  // grows memory.
  bytes(ptr, request.length).set(request);
  // The module takes ownership of these bytes and frees them itself.
  wasm.offthread_on_message(ptr, request.length);
}

// `instantiateStreaming` where the server sends `application/wasm`, falling
// back for hosts that do not. The fallback is not hypothetical: a dev server
// serving `.wasm` as `application/octet-stream` fails the streaming path with
// an error that names MIME types rather than anything useful.
async function boot() {
  const url = "__OFFTHREAD_WASM__";
  let instance;
  try {
    ({ instance } = await WebAssembly.instantiateStreaming(fetch(url), imports));
  } catch (streamingFailed) {
    const module = await WebAssembly.compile(await (await fetch(url)).arrayBuffer());
    instance = await WebAssembly.instantiate(module, imports);
  }

  wasm = instance.exports;
  // Installs the handler and posts `Ready`, which is what unblocks the page.
  wasm.offthread_start();

  started = true;
  for (const data of pending.splice(0)) {
    dispatch(data);
  }
}

boot().catch((e) => {
  // No `Ready` will ever arrive, so the page's handshake is what reports this
  // to the user; this line is for whoever opens the worker's console.
  console.error("offthread: the service failed to start", e);
  // ...and then THROW, from a task, so the failure reaches the OWNING PAGE.
  //
  // A `catch` that only logs leaves the worker alive and silent: the page sees
  // neither `Ready` nor `error`, and a call made before the handshake waits
  // forever. An uncaught throw in a worker task fires `error` on the owning
  // `Worker`, which is the one signal every page half already handles.
  setTimeout(() => {
    throw e;
  }, 0);
});
