//! Package a macroquad app as an Android APK.
//!
//! The Android twin of `ui/macroquad-bundle`: a library, plus a one-line
//! binary in each consuming repo (`tools/mq-apk`) whose `main` is [`main`],
//! because cargo only runs binaries of the workspace it is in.
//!
//! Nothing about one app is compiled in. The Android package, the library the
//! activity loads, the cargo package that holds it, the manifest, the art and
//! the output file are all inputs. What is shared is the part that is the same
//! for every macroquad app: miniquad's Java activity and its `//%` markers,
//! the manifest check, the two-ABI build, the SDK tool order, and the debug
//! key.
//!
//! ```text
//! mq-apk --package au.defrag.x --library x_mq --crate x-mq \
//!        [--manifest <path>] [--icon <png> --banner <png> | --no-art] \
//!        [--abi <abi>]... [--out <path>] [--dry-run]
//! ```
//!
//! `--dry-run` resolves everything, prints the rendered files' provenance and
//! every step, and runs nothing: no javac, no signing, no SDK. Run for real
//! inside the android devshell, where `ANDROID_HOME`, the build-tools and both
//! Rust Android targets come from.

mod activity;
mod build;
mod manifest;
mod metadata;

use build::{Abi, Art, Build};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
usage: mq-apk --package <android.package> --library <lib_name> --crate <cargo-package>
              [--manifest <AndroidManifest.xml>]
              [--icon <icon.png> --banner <banner.png> | --no-art]
              [--abi <abi>]... [--out <file.apk>] [--dry-run]

  --package   the app's Android package; the manifest must agree
  --library   the cdylib's name: lib<library>.so, which MainActivity loads
  --crate     the cargo package that holds the app's library target
  --manifest  defaults to <crate>/android/AndroidManifest.xml
  --icon      the launcher icon, xxxhdpi (192x192)
  --banner    the TV banner, xhdpi (320x180)
  --no-art    package no resources; the manifest must reference none
  --abi       arm64-v8a or armeabi-v7a (repeatable; both by default)
  --out       defaults to <target>/android/<package>.apk
  --dry-run   resolve and print every step; runs nothing and needs no SDK";

#[derive(Debug)]
pub(crate) struct Options {
    pub(crate) package: String,
    pub(crate) library: String,
    pub(crate) krate: String,
    pub(crate) manifest: Option<PathBuf>,
    pub(crate) icon: Option<PathBuf>,
    pub(crate) banner: Option<PathBuf>,
    pub(crate) out: Option<PathBuf>,
    /// The ABIs to package, in plan order. `parse` fills this with every ABI
    /// when none is named.
    pub(crate) abis: Vec<Abi>,
    pub(crate) dry_run: bool,
    pub(crate) cwd: PathBuf,
}

impl Options {
    /// The art to package, with paths resolved against the working directory.
    /// `parse` has already refused a half-given or contradicted pair.
    pub(crate) fn art(&self) -> Result<Option<Art>, String> {
        match (&self.icon, &self.banner) {
            (Some(icon), Some(banner)) => Ok(Some(Art {
                icon: absolute(&self.cwd, icon),
                banner: absolute(&self.cwd, banner),
            })),
            (None, None) => Ok(None),
            _ => Err("--icon and --banner are used together".to_string()),
        }
    }
}

/// The `mq-apk` command: a consuming repo's `tools/mq-apk/src/main.rs` is
/// `fn main() -> std::process::ExitCode { macroquad_apk::main() }`.
pub fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match parse(&args, cwd).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is_empty() => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("✗ {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(opts: Options) -> Result<(), String> {
    let build = Build::resolve(&opts)?;
    let steps = build.plan();
    if opts.dry_run {
        build.describe();
        println!();
        println!("plan:");
        for (index, step) in steps.iter().enumerate() {
            println!("{:>3}. {step}", index + 1);
        }
        println!();
        println!("dry run — {} steps, nothing was run", steps.len());
        return Ok(());
    }
    build::execute(&steps)?;
    build.report()
}

fn parse(args: &[String], cwd: PathBuf) -> Result<Options, String> {
    let (mut package, mut library, mut krate) = (None, None, None);
    let (mut manifest, mut icon, mut banner, mut out) = (None, None, None, None);
    let mut abis: Vec<Abi> = Vec::new();
    let mut no_art = false;
    let mut dry_run = false;
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--package" => package = Some(value()?),
            "--library" => library = Some(value()?),
            "--crate" => krate = Some(value()?),
            "--manifest" => manifest = Some(PathBuf::from(value()?)),
            "--icon" => icon = Some(PathBuf::from(value()?)),
            "--banner" => banner = Some(PathBuf::from(value()?)),
            "--abi" => {
                let abi = Abi::parse(&value()?)?;
                if !abis.contains(&abi) {
                    abis.push(abi);
                }
            }
            "--out" => out = Some(PathBuf::from(value()?)),
            "--no-art" => no_art = true,
            "--dry-run" => dry_run = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument {other}")),
        }
    }

    fn need<T>(value: Option<T>, name: &str) -> Result<T, String> {
        value.ok_or_else(|| format!("--{name} is required"))
    }
    let package = need(package, "package")?;
    let library = need(library, "library")?;
    let krate = need(krate, "crate")?;
    validate_package(&package)?;
    validate_library(&library)?;
    validate_crate(&krate)?;

    if no_art && (icon.is_some() || banner.is_some()) {
        return Err("--no-art cannot be combined with --icon or --banner".to_string());
    }
    match (&icon, &banner) {
        (Some(_), None) => return Err("--icon needs --banner too".to_string()),
        (None, Some(_)) => return Err("--banner needs --icon too".to_string()),
        _ => {}
    }
    if !no_art && icon.is_none() {
        return Err("pass --icon and --banner, or --no-art".to_string());
    }
    if abis.is_empty() {
        abis.extend(Abi::ALL);
    }

    Ok(Options {
        package,
        library,
        krate,
        manifest,
        icon,
        banner,
        out,
        abis,
        dry_run,
        cwd,
    })
}

/// A Java package name: dot-separated identifiers. Validated because it
/// becomes a directory under `work/java` as well as a Java `package` line.
fn validate_package(package: &str) -> Result<(), String> {
    let identifier = |segment: &str| {
        let mut chars = segment.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    if !package.is_empty() && package.split('.').all(identifier) {
        Ok(())
    } else {
        Err(format!(
            "--package `{package}` is not a Java package name (dot-separated letters, digits and `_`, not starting with a digit)"
        ))
    }
}

/// The name `System.loadLibrary` takes: no `lib` prefix, no `.so`.
fn validate_library(library: &str) -> Result<(), String> {
    let mut chars = library.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if valid {
        Ok(())
    } else {
        Err(format!(
            "--library `{library}` is not a C identifier — give the name after `lib` (e.g. `x_mq`, not `libx_mq.so`)"
        ))
    }
}

fn validate_crate(krate: &str) -> Result<(), String> {
    let mut chars = krate.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(format!("--crate `{krate}` is not a cargo package name"))
    }
}

pub(crate) fn absolute(cwd: &std::path::Path, path: &std::path::Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    fn base() -> Vec<String> {
        args(&[
            "--package",
            "au.defrag.example",
            "--library",
            "example_mq",
            "--crate",
            "example-mq",
            "--no-art",
        ])
    }

    #[test]
    fn parses_a_complete_run() {
        let opts = parse(&base(), PathBuf::from("/w")).expect("parses");
        assert_eq!(opts.package, "au.defrag.example");
        assert_eq!(opts.library, "example_mq");
        assert_eq!(opts.krate, "example-mq");
        assert!(opts.art().expect("no art").is_none());
        assert_eq!(opts.abis, Abi::ALL);
        assert_eq!(opts.cwd, PathBuf::from("/w"));
    }

    #[test]
    fn abis_are_opt_in_and_deduplicated() {
        let mut list = base();
        list.extend(args(&["--abi", "armeabi-v7a", "--abi", "arm64-v8a"]));
        let opts = parse(&list, PathBuf::new()).expect("parses");
        assert_eq!(opts.abis, vec![Abi::Armv7, Abi::Arm64]);

        let mut list = base();
        list.extend(args(&[
            "--abi",
            "arm64-v8a",
            "--abi",
            "aarch64-linux-android",
        ]));
        let opts = parse(&list, PathBuf::new()).expect("parses");
        assert_eq!(opts.abis, vec![Abi::Arm64]);
    }

    #[test]
    fn an_unknown_abi_is_named() {
        let mut list = base();
        list.extend(args(&["--abi", "x86_64"]));
        let err = parse(&list, PathBuf::new()).expect_err("unknown abi");
        assert!(err.contains("x86_64"), "{err}");
    }

    #[test]
    fn art_paths_are_resolved_against_the_working_directory() {
        let mut list = base();
        list.retain(|a| a != "--no-art");
        list.extend(args(&[
            "--icon",
            "art/icon.png",
            "--banner",
            "art/banner.png",
        ]));
        let opts = parse(&list, PathBuf::from("/w")).expect("parses");
        let art = opts.art().expect("art").expect("both");
        assert_eq!(art.icon, PathBuf::from("/w/art/icon.png"));
        assert_eq!(art.banner, PathBuf::from("/w/art/banner.png"));
    }

    #[test]
    fn a_half_given_pair_is_refused() {
        let mut list = base();
        list.retain(|a| a != "--no-art");
        list.extend(args(&["--icon", "icon.png"]));
        let err = parse(&list, PathBuf::new()).expect_err("no banner");
        assert_eq!(err, "--icon needs --banner too");
    }

    #[test]
    fn no_art_and_art_together_is_refused() {
        let mut list = base();
        list.extend(args(&["--icon", "icon.png", "--banner", "banner.png"]));
        let err = parse(&list, PathBuf::new()).expect_err("contradiction");
        assert!(err.contains("--no-art"), "{err}");
    }

    #[test]
    fn art_must_be_chosen_explicitly() {
        let mut list = base();
        list.retain(|a| a != "--no-art");
        let err = parse(&list, PathBuf::new()).expect_err("no choice");
        assert!(err.contains("--icon and --banner"), "{err}");
    }

    #[test]
    fn a_missing_flag_is_named() {
        let err = parse(&args(&["--package", "a.b"]), PathBuf::new()).expect_err("missing");
        assert_eq!(err, "--library is required");
    }

    #[test]
    fn a_package_name_cannot_escape_the_work_directory() {
        for bad in ["", "a..b", ".a", "a.", "a/b", "../a", "a.b-c", "1a.b"] {
            let mut list = base();
            list[1] = bad.to_string();
            assert!(parse(&list, PathBuf::new()).is_err(), "{bad} parsed");
        }
    }

    #[test]
    fn the_library_is_the_name_load_library_takes() {
        for bad in ["libx.so", "x-mq", "", "../x"] {
            let mut list = base();
            list[3] = bad.to_string();
            assert!(parse(&list, PathBuf::new()).is_err(), "{bad} parsed");
        }
    }

    #[test]
    fn help_is_an_empty_error_so_main_prints_usage() {
        assert_eq!(parse(&args(&["--help"]), PathBuf::new()).unwrap_err(), "");
    }
}
