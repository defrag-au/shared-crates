use cfg_if::cfg_if;
use wasm_bindgen::JsValue;
use worker::{Queue, QueueContentType, RawMessageBuilder, Result, SendMessage};

mod r2_notification;

pub mod sleep;
pub use r2_notification::*;

pub async fn send_to_queue<M>(queue: &Queue, message: &M) -> Result<()>
where
    M: Into<JsValue> + Clone,
{
    let raw_message = RawMessageBuilder::new(message.clone().into())
        .build_with_content_type(QueueContentType::Json);

    queue.send_raw(raw_message).await
}

pub async fn send_batch_to_queue<M>(queue: &Queue, messages: &[M]) -> Result<()>
where
    M: Into<JsValue> + Clone,
{
    let raw_messages: Vec<SendMessage<JsValue>> = messages
        .iter()
        .map(|m| {
            let js_value = m.clone().into();
            RawMessageBuilder::new(js_value).build_with_content_type(QueueContentType::Json)
        })
        .collect();

    queue.send_raw_batch(raw_messages).await
}

cfg_if! {
    // https://github.com/rustwasm/console_error_panic_hook#readme
    if #[cfg(feature = "console_error_panic_hook")] {
        extern crate console_error_panic_hook;
        pub use self::console_error_panic_hook::set_once as set_panic_hook;
    } else {
        #[inline]
        pub fn set_panic_hook() {}
    }
}

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        use tracing_subscriber::fmt::format::Pretty;
        use tracing_subscriber::fmt::time::UtcTime;
        use tracing_subscriber::prelude::*;
        use tracing_web::{performance_layer, MakeConsoleWriter};

        // For Cloudflare Worker (Wasm)
        pub fn init_tracing(target_level: Option<tracing::Level>) {
            let level = target_level.unwrap_or(tracing::Level::INFO);
            let fmt_layer = tracing_subscriber::fmt::layer()
                    .json()
                    .with_ansi(false) // Only partially supported across JavaScript runtimes
                    .with_timer(UtcTime::rfc_3339()) // std::time is not available in browsers
                    .with_writer(MakeConsoleWriter)
                    .with_filter(tracing_subscriber::filter::LevelFilter::from_level(level));

            let perf_layer = performance_layer().with_details_from_fields(Pretty::default());
            tracing_subscriber::registry()
                .with(fmt_layer)
                .with(perf_layer)
                .init();
        }
    } else {
        // For native tests (non-Wasm)
        pub fn init_tracing(level: Option<tracing::Level>) {
            // Use a simple formatting subscriber for local dev/test logs.
            let subscriber = tracing_subscriber::fmt()
                .with_max_level(level.unwrap_or(tracing::Level::INFO))
                .compact()
                .finish();

            // Set as the default global subscriber (only once!)
            let _ = tracing::subscriber::set_global_default(subscriber);
        }
    }
}
