//! `BackgroundToasts` — declare what work is running; the toasts follow.
//!
//! Every app grows the same three-part dance around a background task: a latch
//! so a fast one doesn't flash a notice, a call to update the progress each
//! frame, and a dismissal when it stops. Written by hand it is a `note_thing`
//! function per task, each with its own flag and its own idea of "long
//! enough", and each of them silently wrong about repaints.
//!
//! Here the app states, every frame, which jobs are *currently running*. This
//! type diffs that against what is on screen and does the rest.
//!
//! ## The repaint problem this exists to solve
//!
//! [`crate::toast::show_toasts`] already drives `request_repaint` while the
//! queue is non-empty, so a toast that EXISTS keeps animating. The gap is the
//! settle delay: during it there is no toast, so nothing is asking for
//! frames. On a surface that idles at one repaint a second, a notice held back
//! for 400ms appears late — or, if the work finishes first, never, which looks
//! like the delay is broken rather than working.
//!
//! [`BackgroundToasts::sync`] schedules a repaint for the exact moment a
//! held-back job becomes due, so the delay means what it says regardless of
//! the host's idle cadence.
//!
//! ## Why a settle delay at all
//!
//! A notice that appears and vanishes inside a second reads as a glitch, not
//! as information. Work that finishes quickly should finish silently.
//!
//! ## Completion is not news
//!
//! A job that disappears from the live set is dismissed quietly. Nobody asked
//! for background work, so its finishing is not an event — if an outcome IS
//! worth reporting (an excavation that found something, a save that failed),
//! that is a `ToastQueue::resolve` at the call site, not this.
//!
//! ```
//! use egui_widgets::background::{BackgroundToasts, Job};
//!
//! let mut bg = BackgroundToasts::new().settle_after(0.4);
//!
//! // Frame 1: work starts. Too soon to say anything, but we know when to look
//! // again.
//! let plan = bg.plan(0.0, &[Job::new("naming", "naming wallets — 0/231")]);
//! assert!(plan.show.is_empty());
//! assert_eq!(plan.wake_after, Some(0.4));
//!
//! // Still running once the delay has passed: now it is worth saying.
//! let plan = bg.plan(0.5, &[Job::new("naming", "naming wallets — 40/231")]);
//! assert_eq!(plan.show.len(), 1);
//!
//! // Gone from the live set: dismissed, without a completion notice.
//! let plan = bg.plan(0.6, &[]);
//! assert_eq!(plan.dismiss, ["naming"]);
//! ```

use std::collections::HashMap;

use egui::Context;

use crate::toast::ToastQueue;

/// How long a job must run before it is worth mentioning.
const DEFAULT_SETTLE_SECS: f64 = 0.4;

/// One unit of background work, as it stands right now.
///
/// Built fresh each frame from whatever the app can observe — a queue depth, a
/// `Loading` state — rather than tracked. There is no "start" or "finish" call
/// to forget, and no flag to fall out of step with the thing it describes.
pub struct Job<'a> {
    pub key: &'a str,
    pub message: String,
    /// `None` for indeterminate work. Use it: inventing a fraction for a
    /// single request with no count behind it is a claim about progress the
    /// app cannot make.
    pub progress: Option<f32>,
}

impl<'a> Job<'a> {
    pub fn new(key: &'a str, message: impl Into<String>) -> Self {
        Self {
            key,
            message: message.into(),
            progress: None,
        }
    }

    pub fn progress(mut self, fraction: Option<f32>) -> Self {
        self.progress = fraction;
        self
    }
}

/// What [`BackgroundToasts::plan`] decided, as data — so the decision can be
/// tested without a UI, a clock, or a frame loop.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    /// Toasts to upsert: `(key, message, progress)`.
    pub show: Vec<(String, String, Option<f32>)>,
    /// Toasts to take down.
    pub dismiss: Vec<String>,
    /// Seconds until the soonest held-back job comes due, if any. The caller
    /// must ensure a frame happens by then or the notice arrives late.
    pub wake_after: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct BackgroundToasts {
    /// key → when it was first seen running.
    first_seen: HashMap<String, f64>,
    /// Keys currently on screen, so a dismissal happens once.
    showing: Vec<String>,
    settle: f64,
}

impl Default for BackgroundToasts {
    fn default() -> Self {
        Self::new()
    }
}

impl BackgroundToasts {
    pub fn new() -> Self {
        Self {
            first_seen: HashMap::new(),
            showing: Vec::new(),
            settle: DEFAULT_SETTLE_SECS,
        }
    }

    /// How long a job must run before it earns a notice. Zero shows
    /// immediately, for work that is always slow enough to be worth naming.
    pub fn settle_after(mut self, secs: f64) -> Self {
        self.settle = secs.max(0.0);
        self
    }

    /// Decide what should change, given the wall-clock and the jobs running
    /// right now. Pure: same inputs, same plan.
    pub fn plan(&mut self, now: f64, live: &[Job<'_>]) -> Plan {
        let mut plan = Plan::default();

        for job in live {
            let first = *self.first_seen.entry(job.key.to_string()).or_insert(now);
            let elapsed = now - first;
            if elapsed >= self.settle {
                plan.show
                    .push((job.key.to_string(), job.message.clone(), job.progress));
                if !self.showing.iter().any(|k| k == job.key) {
                    self.showing.push(job.key.to_string());
                }
            } else {
                // Not due yet. Whoever drives the frames needs to come back at
                // the moment it is, or the delay silently becomes "however
                // long until something else causes a repaint".
                let remaining = self.settle - elapsed;
                plan.wake_after = Some(match plan.wake_after {
                    Some(w) => w.min(remaining),
                    None => remaining,
                });
            }
        }

        // Anything that stopped running comes down — quietly.
        self.showing.retain(|key| {
            let still_live = live.iter().any(|j| j.key == key);
            if !still_live {
                plan.dismiss.push(key.clone());
            }
            still_live
        });
        self.first_seen
            .retain(|key, _| live.iter().any(|j| j.key == key));

        plan
    }

    /// Reconcile against the live set and apply the result: upsert the toasts
    /// that are due, take down the ones whose work has stopped, and schedule a
    /// repaint for the next job that becomes due.
    pub fn sync(&mut self, ctx: &Context, toasts: &mut ToastQueue, live: &[Job<'_>]) {
        let now = ctx.input(|i| i.time);
        let plan = self.plan(now, live);
        for (key, message, progress) in plan.show {
            toasts.progress(key, message, progress);
        }
        for key in plan.dismiss {
            toasts.dismiss(&key);
        }
        if let Some(after) = plan.wake_after {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(after));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(key: &str) -> Job<'_> {
        Job::new(key, "working")
    }

    #[test]
    fn quick_work_is_never_announced() {
        let mut bg = BackgroundToasts::new().settle_after(0.4);
        assert!(bg.plan(0.0, &[job("a")]).show.is_empty());
        // Finished at 0.2s — under the delay, so nothing was ever said, and
        // there is nothing to take down either.
        let plan = bg.plan(0.2, &[]);
        assert!(plan.show.is_empty());
        assert!(
            plan.dismiss.is_empty(),
            "a notice never shown needs no dismissal"
        );
    }

    #[test]
    fn work_that_outlasts_the_delay_is_announced() {
        let mut bg = BackgroundToasts::new().settle_after(0.4);
        bg.plan(0.0, &[job("a")]);
        let plan = bg.plan(0.41, &[job("a")]);
        assert_eq!(plan.show.len(), 1);
        assert_eq!(plan.show[0].0, "a");
    }

    /// The bug this type exists to prevent: during the settle delay there is
    /// no toast, so nothing is driving repaints. Without a scheduled wake-up
    /// the notice appears whenever the host next happens to redraw — a second
    /// later on an idle page.
    #[test]
    fn a_held_back_job_schedules_its_own_wake_up() {
        let mut bg = BackgroundToasts::new().settle_after(0.4);
        assert_eq!(bg.plan(0.0, &[job("a")]).wake_after, Some(0.4));
        // Partway through, ask for the remainder rather than the whole delay.
        let plan = bg.plan(0.25, &[job("a")]);
        assert!((plan.wake_after.expect("still waiting") - 0.15).abs() < 1e-9);
    }

    #[test]
    fn the_soonest_due_job_sets_the_wake_up() {
        let mut bg = BackgroundToasts::new().settle_after(1.0);
        bg.plan(0.0, &[job("slow")]);
        // `quick` starts later, so `slow` comes due first.
        let plan = bg.plan(0.5, &[job("slow"), job("quick")]);
        assert!((plan.wake_after.expect("waiting") - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_shown_job_needs_no_further_wake_up() {
        let mut bg = BackgroundToasts::new().settle_after(0.4);
        bg.plan(0.0, &[job("a")]);
        let plan = bg.plan(1.0, &[job("a")]);
        assert!(!plan.show.is_empty());
        assert_eq!(
            plan.wake_after, None,
            "show_toasts drives repaints once a toast exists"
        );
    }

    #[test]
    fn a_finished_job_is_dismissed_once() {
        let mut bg = BackgroundToasts::new().settle_after(0.0);
        bg.plan(0.0, &[job("a")]);
        assert_eq!(bg.plan(1.0, &[]).dismiss, ["a"]);
        assert!(
            bg.plan(2.0, &[]).dismiss.is_empty(),
            "dismissing twice would fight anything else using that key"
        );
    }

    /// Restarting means restarting: a job that stops and starts again serves
    /// its delay afresh rather than snapping back instantly.
    #[test]
    fn a_restarted_job_serves_the_delay_again() {
        let mut bg = BackgroundToasts::new().settle_after(0.4);
        bg.plan(0.0, &[job("a")]);
        bg.plan(0.5, &[job("a")]); // shown
        bg.plan(0.6, &[]); // stopped
        let plan = bg.plan(0.7, &[job("a")]);
        assert!(plan.show.is_empty(), "the clock restarts with the work");
        assert_eq!(plan.wake_after, Some(0.4));
    }

    #[test]
    fn several_jobs_are_tracked_independently() {
        let mut bg = BackgroundToasts::new().settle_after(0.4);
        bg.plan(0.0, &[job("a")]);
        let plan = bg.plan(0.5, &[job("a"), job("b")]);
        assert_eq!(plan.show.len(), 1, "only `a` has served its delay");
        assert_eq!(plan.show[0].0, "a");
        assert_eq!(plan.wake_after, Some(0.4), "`b` is still waiting");
    }

    #[test]
    fn progress_is_carried_through_untouched() {
        let mut bg = BackgroundToasts::new().settle_after(0.0);
        let plan = bg.plan(0.0, &[Job::new("a", "half").progress(Some(0.5))]);
        assert_eq!(plan.show[0].2, Some(0.5));
        let plan = bg.plan(1.0, &[Job::new("a", "unknown").progress(None)]);
        assert_eq!(plan.show[0].2, None, "indeterminate stays indeterminate");
    }
}
