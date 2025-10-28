use worker_stack::js_sys;
use worker_stack::wasm_bindgen;

#[worker_stack::wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = "setTimeout")]
    fn set_timeout(cb: &js_sys::Function, delay: i32);
}

pub async fn sleep(delay: i32) {
    let mut cb =
        |resolve: js_sys::Function, _reject: js_sys::Function| set_timeout(&resolve, delay);

    let p = js_sys::Promise::new(&mut cb);

    worker_stack::wasm_bindgen_futures::JsFuture::from(p)
        .await
        .unwrap_or_else(|e| {
            eprintln!("An error occurred awaiting JS future: {e:?}");
            Default::default()
        });
}
