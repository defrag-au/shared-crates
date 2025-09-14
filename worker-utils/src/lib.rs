use cfg_if::cfg_if;
use serde::Serialize;
use wasm_bindgen::JsValue;
use worker::{Queue, QueueContentType, RawMessageBuilder, Result, SendMessage};

mod r2_notification;

pub mod sleep;
pub use r2_notification::*;

#[cfg(feature = "axum")]
pub mod axum;

#[cfg(feature = "axum")]
pub use axum::*;

pub async fn send_to_queue<M>(queue: &Queue, message: &M) -> Result<()>
where
    M: Serialize + Clone,
{
    // Use JSON-compatible serializer to handle HashMap properly (fixes HashMap<String, Vec<String>> serialization)
    let serializer = serde_wasm_bindgen::Serializer::json_compatible();
    let js_value = message
        .serialize(&serializer)
        .map_err(|e| worker::Error::RustError(format!("Serialization failed: {e}")))?;

    let raw_message =
        RawMessageBuilder::new(js_value).build_with_content_type(QueueContentType::Json);

    queue.send_raw(raw_message).await
}

pub async fn send_batch_to_queue<M>(queue: &Queue, messages: &[M]) -> Result<()>
where
    M: Serialize + Clone,
{
    let raw_messages: Result<Vec<SendMessage<JsValue>>> = messages
        .iter()
        .map(|m| {
            // Use JSON-compatible serializer to handle HashMap properly
            let serializer = serde_wasm_bindgen::Serializer::json_compatible();
            let js_value = m
                .serialize(&serializer)
                .map_err(|e| worker::Error::RustError(format!("Serialization failed: {e}")))?;
            Ok(RawMessageBuilder::new(js_value).build_with_content_type(QueueContentType::Json))
        })
        .collect();

    queue.send_raw_batch(raw_messages?).await
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

        #[cfg(feature = "simple-logging")]
        pub fn init_tracing(target_level: Option<tracing::Level>) {
            use tracing::{Event, Metadata, Subscriber};
            use tracing::subscriber::set_global_default;

            struct SimpleLogger {
                max_level: tracing::Level,
            }

            impl Subscriber for SimpleLogger {
                fn enabled(&self, metadata: &Metadata<'_>) -> bool {
                    metadata.level() <= &self.max_level
                }

                fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
                    tracing::span::Id::from_u64(1)
                }

                fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

                fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

                fn enter(&self, _span: &tracing::span::Id) {}

                fn exit(&self, _span: &tracing::span::Id) {}

                fn event(&self, event: &Event<'_>) {
                    if self.enabled(event.metadata()) {
                        let level = event.metadata().level();
                        let level_str = match *level {
                            tracing::Level::ERROR => "ERROR",
                            tracing::Level::WARN => "WARN",
                            tracing::Level::INFO => "INFO",
                            tracing::Level::DEBUG => "DEBUG",
                            tracing::Level::TRACE => "TRACE",
                        };

                        // Format the message
                        let mut visitor = MessageVisitor::new();
                        event.record(&mut visitor);

                        let log_line = format!("{} {}", level_str, visitor.message);

                        // Use appropriate console method based on level
                        match *level {
                            tracing::Level::ERROR => web_sys::console::error_1(&log_line.into()),
                            tracing::Level::WARN => web_sys::console::warn_1(&log_line.into()),
                            _ => web_sys::console::log_1(&log_line.into()),
                        }
                    }
                }
            }

            struct MessageVisitor {
                message: String,
            }

            impl MessageVisitor {
                fn new() -> Self {
                    Self { message: String::new() }
                }
            }

            impl tracing::field::Visit for MessageVisitor {
                fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                    if field.name() == "message" {
                        self.message = format!("{:?}", value).trim_matches('"').to_string();
                    } else {
                        if !self.message.is_empty() {
                            self.message.push(' ');
                        }
                        self.message.push_str(&format!("{}={:?}", field.name(), value));
                    }
                }
            }

            let level = target_level.unwrap_or(tracing::Level::INFO);
            let logger = SimpleLogger { max_level: level };

            let _ = set_global_default(logger);
        }

        #[cfg(feature = "full-logging")]
        pub fn init_tracing(target_level: Option<tracing::Level>) {
            use tracing_subscriber::fmt::format::Pretty;
            use tracing_subscriber::fmt::time::UtcTime;
            use tracing_subscriber::prelude::*;
            use tracing_web::{performance_layer, MakeConsoleWriter};

            let level = target_level.unwrap_or(tracing::Level::INFO);

            // Use compact format instead of JSON for better readability
            let fmt_layer = tracing_subscriber::fmt::layer()
                    .compact()
                    .with_ansi(false)
                    .with_timer(UtcTime::rfc_3339())
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
        #[allow(unused_variables)]
        pub fn init_tracing(level: Option<tracing::Level>) {
            #[cfg(feature = "full-logging")]
            {
                use tracing_subscriber;
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
}
