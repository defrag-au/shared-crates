//! Scheduled job dispatch pattern for Cloudflare Workers.
//!
//! Provides a trait and runner for the common pattern of mapping cron expressions
//! to job enum variants. The `#[event(scheduled)]` macro must live in each worker
//! (it generates wasm bindings), but this module standardises the dispatch logic.
//!
//! # Usage
//!
//! ```rust,ignore
//! use phf::phf_map;
//!
//! #[derive(Debug, Clone)]
//! enum Job {
//!     SyncData,
//!     Cleanup,
//! }
//!
//! impl worker_utils::scheduled::ScheduledJob for Job {
//!     fn from_cron(cron: &str) -> Option<Self> {
//!         static SCHEDULES: phf::Map<&str, Job> = phf_map! {
//!             "*/5 * * * *" => Job::SyncData,
//!             "0 0 * * *" => Job::Cleanup,
//!         };
//!         SCHEDULES.get(cron).cloned()
//!     }
//! }
//!
//! #[event(scheduled, respond_with_errors)]
//! pub async fn scheduled(event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
//!     worker_utils::scheduled::dispatch::<Job, _, _>(&event, |job| async {
//!         match job {
//!             Job::SyncData => sync_data(&env).await,
//!             Job::Cleanup => cleanup(&env).await,
//!         }
//!     }).await;
//! }
//! ```

pub use phf;

use worker_stack::worker::ScheduledEvent;

/// Trait for scheduled job enums.
///
/// Implement this on your job enum to map cron expressions to job variants.
/// Use `phf_map!` for O(1) lookup.
pub trait ScheduledJob: std::fmt::Debug + Clone {
    /// Look up which job matches the given cron expression.
    fn from_cron(cron: &str) -> Option<Self>;
}

/// Dispatch a scheduled event to the appropriate job handler.
///
/// Looks up the job from the cron expression, logs the dispatch, and runs the handler.
/// Errors are logged but not propagated (scheduled handlers can't return errors to the runtime).
pub async fn dispatch<J, F, Fut>(event: &ScheduledEvent, handler: F)
where
    J: ScheduledJob,
    F: FnOnce(J) -> Fut,
    Fut: std::future::Future<Output = Result<(), Box<dyn std::error::Error>>>,
{
    let cron = event.cron();
    match J::from_cron(&cron) {
        Some(job) => {
            let job_debug = format!("{job:?}");
            tracing::info!(cron = %cron, job = %job_debug, "Running scheduled job");
            if let Err(e) = handler(job).await {
                tracing::error!(cron = %cron, job = %job_debug, error = %e, "Scheduled job failed");
            }
        }
        None => {
            tracing::warn!(cron = %cron, "No job registered for cron schedule");
        }
    }
}
