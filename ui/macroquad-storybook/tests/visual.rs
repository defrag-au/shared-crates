//! Golden tests for the storybook's visual corpus.
//!
//! The storybook is the corpus: every story is a widget or a composed screen,
//! and `--shot` fixes the clock at 1/60 s for the capture. `--list` gives this
//! test the same slugs the sidebar and `--story` use, so a story cannot exist
//! without appearing here.
//!
//! One golden per story per platform: `tests/goldens/<slug>/<os>.png`. The
//! renderer is the contract — Metal and llvmpipe round differently — so a
//! platform without its own goldens fails rather than borrowing another's.
//!
//! Each story declares the canvas it is drawn for (`Story::canvas`), and
//! `--no-chrome` captures exactly that plus a margin. A widget's golden is
//! its own size, not the storybook window's, and the storybook's header or
//! sidebar cannot invalidate it — neither is drawn.
//!
//! Two checks guard a story. The golden asks whether the picture changed; the
//! capture's text report asks whether it is wrong — text off the story's
//! canvas, cut short, below 8 pt, or overlapping. A bless can silence the
//! first but not the second.
//!
//! A responsive composition gets one story per breakpoint arm, not a story per
//! width. `app-shell/shell-phone` (390×844: bottom bar and phone insets) and
//! `app-shell/shell-desk` (1280×800: rail and no insets) share one body, and
//! `shell_layout` chooses between them from the canvas width. That pair is the
//! worked example: two goldens cover both compositions, and a third width is
//! added only when it lays out differently.
//!
//! Bless a batch with:
//!
//! ```sh
//! UPDATE_GOLDENS=1 cargo test -p macroquad-storybook --test visual
//! ```
//!
//! The categories below are the ones stable enough to pin. A category joins
//! the list once its view models stop moving under feature work; every story
//! in a listed category is then expected to have a golden.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

const CATEGORIES: &[&str] = &["atoms", "input", "gamekit", "page", "app-shell", "admin"];

/// Stories that exist to show a cut: the app bar's over-long title and the card
/// face's shrinking name plate. `cut` is the only problem they may report, and
/// any other problem in either still fails.
const EXPECTED_CUTS: &[&str] = &["app-shell/app-bar", "gamekit/card-face"];

fn storybook() -> &'static str {
    env!("CARGO_BIN_EXE_macroquad-storybook")
}

fn goldens_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens")
}

/// `tests/goldens/<slug>/<os>.png`.
fn golden(slug: &str) -> PathBuf {
    goldens_root()
        .join(slug)
        .join(format!("{}.png", std::env::consts::OS))
}

fn scratch() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("visual");
    std::fs::create_dir_all(&dir).expect("create the visual scratch directory");
    dir
}

/// `(slug, category, capture time)` for every story the storybook lists.
fn listed() -> Vec<(String, String, Option<f64>)> {
    let out = Command::new(storybook())
        .arg("--list")
        .output()
        .expect("run the storybook with --list");
    assert!(
        out.status.success(),
        "--list exited {:?}\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("--list prints UTF-8");
    let stories: Vec<(String, String, Option<f64>)> = text
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            let slug = fields.first()?.trim();
            let category = slug.split('/').next()?;
            let at = fields
                .get(2)
                .and_then(|f| f.strip_prefix("at="))
                .and_then(|s| s.parse().ok());
            (!slug.is_empty()).then(|| (slug.to_owned(), category.to_owned(), at))
        })
        .collect();
    assert!(!stories.is_empty(), "--list printed no stories");
    stories
}

fn selected() -> Vec<(String, Option<f64>)> {
    listed()
        .into_iter()
        .filter(|(_, category, _)| CATEGORIES.contains(&category.as_str()))
        .map(|(slug, _, at)| (slug, at))
        .collect()
}

#[test]
fn every_selected_story_has_a_golden() {
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        return;
    }
    let missing: Vec<_> = selected()
        .into_iter()
        .map(|(slug, _)| slug)
        .filter(|slug| !golden(slug).exists())
        .collect();
    assert!(
        missing.is_empty(),
        "no golden for {} selected stories: {missing:#?}\n\
         run UPDATE_GOLDENS=1 cargo test -p macroquad-storybook --test visual",
        missing.len()
    );
}

#[test]
fn every_selected_story_matches_its_golden() {
    let dir = scratch();
    let slugs = selected();
    let mut failures = Vec::new();
    for (slug, at) in &slugs {
        let shot = dir.join(format!("{}.png", slug.replace('/', "-")));
        let mut command = Command::new(storybook());
        command.args([
            "--story",
            slug,
            "--shot",
            shot.to_str().expect("scratch path is UTF-8"),
            "--check",
            golden(slug)
                .parent()
                .expect("a golden has a directory")
                .to_str()
                .expect("golden path is UTF-8"),
            // The story's own canvas, without the storybook's header or
            // sidebar: a change to either cannot invalidate a golden. No
            // `--size`: the window is not what a golden is sized by.
            "--no-chrome",
            "--tolerance",
            "2",
        ]);
        if let Some(at) = at {
            command.arg("--at").arg(at.to_string());
        }
        let out = command
            .output()
            .unwrap_or_else(|err| panic!("run the storybook for {slug}: {err}"));
        if !out.status.success() {
            failures.push(format!(
                "{slug} (exit {:?})\nstdout:\n{}\nstderr:\n{}",
                out.status.code(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} selected stories differ:\n\n{}",
        failures.len(),
        slugs.len(),
        failures.join("\n\n")
    );
}

#[test]
fn every_selected_story_has_no_text_problems() {
    let dir = scratch().join("problems");
    std::fs::create_dir_all(&dir).expect("create the problem scratch directory");
    let slugs = selected();
    let mut failures = Vec::new();
    for (slug, at) in &slugs {
        let shot = dir.join(format!("{}.png", slug.replace('/', "-")));
        let mut command = Command::new(storybook());
        command.args([
            "--story",
            slug,
            "--shot",
            shot.to_str().expect("scratch path is UTF-8"),
            "--no-chrome",
            "--fail-on-problems",
        ]);
        if let Some(at) = at {
            command.arg("--at").arg(at.to_string());
        }
        if EXPECTED_CUTS.contains(&slug.as_str()) {
            command.args(["--allow", "cut"]);
        }
        let out = command
            .output()
            .unwrap_or_else(|err| panic!("run the storybook for {slug}: {err}"));
        if !out.status.success() {
            let report = std::fs::read_to_string(shot.with_extension("txt")).unwrap_or_default();
            failures.push(format!(
                "{slug} (exit {:?})\nstdout:\n{}\nstderr:\n{}\nreport:\n{report}",
                out.status.code(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} selected stories report text problems:\n\n{}",
        failures.len(),
        slugs.len(),
        failures.join("\n\n")
    );
}

#[test]
fn every_golden_belongs_to_a_story() {
    let known: BTreeSet<String> = listed().into_iter().map(|(slug, _, _)| slug).collect();
    let os = format!("{}.png", std::env::consts::OS);
    let root = goldens_root();
    let mut stray = Vec::new();
    if root.exists() {
        for category in std::fs::read_dir(&root).expect("read tests/goldens") {
            let category = category.expect("directory entry").path();
            if !category.is_dir() {
                continue;
            }
            let cat = category.file_name().unwrap().to_string_lossy().to_string();
            for story in std::fs::read_dir(&category).expect("read a category") {
                let story = story.expect("directory entry").path();
                if !story.is_dir() || !story.join(&os).exists() {
                    continue;
                }
                let name = story.file_name().unwrap().to_string_lossy().to_string();
                let slug = format!("{cat}/{name}");
                if !known.contains(&slug) {
                    stray.push(slug);
                }
            }
        }
    }
    assert!(
        stray.is_empty(),
        "goldens with no story (renamed or removed?): {stray:#?}"
    );
}
