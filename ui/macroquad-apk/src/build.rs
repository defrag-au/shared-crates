//! The build as an ordered list of steps.
//!
//! The list is built before anything runs, so `--dry-run` prints exactly what
//! the real build will execute rather than a prose description written beside
//! it. That also makes the ordering testable without an SDK: the tools are
//! only names and arguments here.
//!
//! The shape is the one `scripts/build-apk.sh` proved:
//!
//! 1. the app as a shared object per ABI (`cargo rustc --crate-type cdylib`),
//!    with symbols stripped but the dynamic table kept — Java finds the entry
//!    points by name
//! 2. miniquad's Java activity rendered for this package, plus `QuadNative`
//! 3. javac → d8 → aapt2 link → zip → zipalign → apksigner
//!
//! Signed with the standard Android debug key so reinstalls keep app data. A
//! release key comes with distribution, not with this tool.

use crate::{Options, activity, manifest, metadata};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

/// One Android ABI an APK can carry. Both are packaged by default: arm64
/// phones, and TVs like the Google TV Streamer that run a 32-bit userspace
/// (`ro.product.cpu.abilist` = armeabi-v7a,armeabi). `--abi` narrows it, which
/// is what a phone-only experiment wants — the second slice is a second full
/// compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Abi {
    Arm64,
    Armv7,
}

impl Abi {
    pub const ALL: [Abi; 2] = [Abi::Arm64, Abi::Armv7];

    /// Accept both the APK directory name and the Rust target name, because
    /// both are already in the reader's head from the build.ini/`.so` paths.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "arm64-v8a" | "aarch64-linux-android" => Ok(Abi::Arm64),
            "armeabi-v7a" | "armv7-linux-androideabi" => Ok(Abi::Armv7),
            other => Err(format!(
                "unknown ABI `{other}` — use arm64-v8a or armeabi-v7a"
            )),
        }
    }

    pub fn target(self) -> &'static str {
        match self {
            Abi::Arm64 => "aarch64-linux-android",
            Abi::Armv7 => "armv7-linux-androideabi",
        }
    }

    pub fn dir(self) -> &'static str {
        match self {
            Abi::Arm64 => "arm64-v8a",
            Abi::Armv7 => "armeabi-v7a",
        }
    }
}

/// Used when the devshell does not export them. They exist so `--dry-run`
/// works outside a shell with no SDK; a real build needs the platform's
/// `android.jar`, so an unset `ANDROID_HOME` is caught before any step runs.
const DEFAULT_PLATFORM: &str = "36";
const DEFAULT_MIN_API: &str = "30";

/// The standard Android debug keystore's password. The key is not a secret —
/// it is the one every development device already trusts for debug builds.
const DEBUG_KEYSTORE_PASSWORD: &str = "android";

/// Everything one packaging run needs, resolved.
pub struct Build {
    package: String,
    library: String,
    krate: String,
    cwd: PathBuf,
    target_dir: PathBuf,
    /// Where the app's manifest came from, for the report.
    manifest_source: PathBuf,
    /// The manifest as it will be packaged.
    manifest: String,
    /// miniquad's activity as it will be compiled.
    activity: String,
    miniquad: String,
    miniquad_java: PathBuf,
    art: Option<Art>,
    abis: Vec<Abi>,
    out: PathBuf,
    sdk: Sdk,
    keystore: PathBuf,
}

/// The launcher icon and TV banner, already drawn.
pub struct Art {
    pub icon: PathBuf,
    pub banner: PathBuf,
}

/// The Android platform being packaged against.
struct Sdk {
    /// `None` only in a dry run: the commands then show `${ANDROID_HOME}`.
    home: Option<PathBuf>,
    platform: String,
    min_api: String,
}

/// One thing the build does, in order.
pub enum Step {
    RemoveDir(PathBuf),
    MakeDir(PathBuf),
    Copy {
        from: PathBuf,
        to: PathBuf,
    },
    Write {
        path: PathBuf,
        contents: String,
        what: &'static str,
    },
    Run(RunStep),
}

pub struct RunStep {
    what: &'static str,
    program: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    /// Appended at execution time: every `.class` under this directory.
    /// javac's output (inner classes included) is not known when the plan is
    /// printed, so the dry run shows `<dir>/*.class` instead.
    class_files: Option<PathBuf>,
}

impl Build {
    /// Resolve a run: the app crate, miniquad's Java, both rendered files, the
    /// output path and the SDK.
    pub fn resolve(opts: &Options) -> Result<Self, String> {
        let resolved = metadata::resolve(&opts.cwd, &opts.krate, metadata::PRIMARY_TARGET)?;

        let manifest_source = match &opts.manifest {
            Some(path) => crate::absolute(&opts.cwd, path),
            None => resolved.crate_dir.join("android/AndroidManifest.xml"),
        };
        let source = fs::read_to_string(&manifest_source)
            .map_err(|e| format!("read {}: {e}", manifest_source.display()))?;
        let manifest = manifest::render(&source, &opts.package)
            .map_err(|e| format!("{}: {e}", manifest_source.display()))?;

        let activity_path = resolved.miniquad_java.join("MainActivity.java");
        let source = fs::read_to_string(&activity_path)
            .map_err(|e| format!("read {}: {e}", activity_path.display()))?;
        let activity = activity::render(&source, &opts.package, &opts.library)
            .map_err(|e| format!("{}: {e}", activity_path.display()))?;
        let quad_native = resolved.miniquad_java.join("QuadNative.java");
        if !quad_native.is_file() {
            return Err(format!(
                "miniquad {} has no QuadNative.java at {}",
                resolved.miniquad,
                quad_native.display()
            ));
        }

        let out = match &opts.out {
            Some(path) => crate::absolute(&opts.cwd, path),
            None => resolved
                .target_dir
                .join("android")
                .join(format!("{}.apk", opts.package)),
        };
        let sdk = Sdk::resolve(opts.dry_run)?;
        let keystore = debug_keystore()?;

        Ok(Self {
            package: opts.package.clone(),
            library: opts.library.clone(),
            krate: opts.krate.clone(),
            cwd: opts.cwd.clone(),
            target_dir: resolved.target_dir,
            manifest_source,
            manifest,
            activity,
            miniquad: resolved.miniquad,
            miniquad_java: resolved.miniquad_java,
            art: opts.art()?,
            abis: opts.abis.clone(),
            out,
            sdk,
            keystore,
        })
    }

    /// What the dry run prints before the steps.
    pub fn describe(&self) {
        println!("mq-apk: {}", self.package);
        println!("  crate     {} → lib{}.so", self.krate, self.library);
        println!("  manifest  {}", self.manifest_source.display());
        println!(
            "  miniquad  {} at {}",
            self.miniquad,
            self.miniquad_java.display()
        );
        match &self.art {
            Some(art) => println!(
                "  art       {} + {}",
                art.icon.display(),
                art.banner.display()
            ),
            None => println!("  art       none"),
        }
        println!(
            "  abi       {}",
            self.abis
                .iter()
                .map(|abi| abi.dir())
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!("  work      {}", self.work().display());
        println!("  out       {}", self.out.display());
        println!("  android   {}", self.sdk.describe());
    }

    /// The build, in order.
    pub fn plan(&self) -> Vec<Step> {
        let work = self.work();
        let java = work.join("java");
        let package_java = java.join(self.package.replace('.', "/"));
        let quad_native = java.join("quad_native");
        let classes = work.join("classes");
        let apk = work.join("apk");
        let res = work.join("res");
        let res_zip = work.join("res.zip");
        let manifest = work.join("AndroidManifest.xml");
        let unaligned = work.join("unaligned.apk");
        let aligned = work.join("aligned.apk");
        let android_jar = self.sdk.android_jar();

        let mut steps = vec![
            Step::RemoveDir(work.clone()),
            Step::MakeDir(package_java.clone()),
            Step::MakeDir(quad_native.clone()),
            Step::MakeDir(classes.clone()),
            Step::MakeDir(apk.clone()),
        ];

        if let Some(art) = &self.art {
            steps.push(Step::MakeDir(res.join("mipmap-xxxhdpi")));
            steps.push(Step::MakeDir(res.join("drawable-xhdpi")));
            steps.push(Step::Copy {
                from: art.icon.clone(),
                to: res.join("mipmap-xxxhdpi/ic_launcher.png"),
            });
            steps.push(Step::Copy {
                from: art.banner.clone(),
                to: res.join("drawable-xhdpi/banner.png"),
            });
        }

        let main_activity = package_java.join("MainActivity.java");
        let quad_java = quad_native.join("QuadNative.java");
        steps.push(Step::Write {
            path: main_activity.clone(),
            contents: self.activity.clone(),
            what: "miniquad's MainActivity.java, rendered",
        });
        steps.push(Step::Copy {
            from: self.miniquad_java.join("QuadNative.java"),
            to: quad_java.clone(),
        });
        steps.push(Step::Write {
            path: manifest.clone(),
            contents: self.manifest.clone(),
            what: "the app's manifest",
        });

        for abi in &self.abis {
            let target = abi.target();
            let args = vec![
                "rustc".to_string(),
                "-p".to_string(),
                self.krate.clone(),
                "--lib".to_string(),
                "--release".to_string(),
                "--target".to_string(),
                target.to_string(),
                "--crate-type".to_string(),
                "cdylib".to_string(),
                "--".to_string(),
                "-C".to_string(),
                "strip=symbols".to_string(),
            ];
            steps.push(Step::Run(RunStep {
                what: "build the shared object",
                program: "cargo".to_string(),
                args,
                cwd: Some(self.cwd.clone()),
                class_files: None,
            }));

            let lib = apk.join("lib").join(abi.dir());
            let name = format!("lib{}.so", self.library);
            steps.push(Step::MakeDir(lib.clone()));
            steps.push(Step::Copy {
                from: self.target_dir.join(target).join("release").join(&name),
                to: lib.join(&name),
            });
        }

        let mut javac = vec![
            "-nowarn".to_string(),
            "-source".to_string(),
            "11".to_string(),
            "-target".to_string(),
            "11".to_string(),
            "-classpath".to_string(),
            android_jar.clone(),
            "-d".to_string(),
            classes.display().to_string(),
        ];
        javac.push(main_activity.display().to_string());
        javac.push(quad_java.display().to_string());
        steps.push(Step::Run(RunStep {
            what: "compile the Java",
            program: "javac".to_string(),
            args: javac,
            cwd: None,
            class_files: None,
        }));

        steps.push(Step::Run(RunStep {
            what: "dex the Java",
            program: "d8".to_string(),
            args: vec![
                "--release".to_string(),
                "--min-api".to_string(),
                self.sdk.min_api.clone(),
                "--lib".to_string(),
                android_jar.clone(),
                "--output".to_string(),
                apk.display().to_string(),
            ],
            cwd: None,
            class_files: Some(classes),
        }));

        if self.art.is_some() {
            steps.push(Step::Run(RunStep {
                what: "compile the resources",
                program: "aapt2".to_string(),
                args: vec![
                    "compile".to_string(),
                    "--dir".to_string(),
                    res.display().to_string(),
                    "-o".to_string(),
                    res_zip.display().to_string(),
                ],
                cwd: None,
                class_files: None,
            }));
        }

        let mut link = vec![
            "link".to_string(),
            "-o".to_string(),
            unaligned.display().to_string(),
            "--manifest".to_string(),
            manifest.display().to_string(),
            "-I".to_string(),
            android_jar,
            "--min-sdk-version".to_string(),
            self.sdk.min_api.clone(),
            "--target-sdk-version".to_string(),
            self.sdk.platform.clone(),
        ];
        if self.art.is_some() {
            link.push(res_zip.display().to_string());
        }
        steps.push(Step::Run(RunStep {
            what: "link the APK",
            program: "aapt2".to_string(),
            args: link,
            cwd: None,
            class_files: None,
        }));

        steps.push(Step::Run(RunStep {
            what: "add the dex and the native libraries",
            program: "zip".to_string(),
            args: vec![
                "-q".to_string(),
                "-r".to_string(),
                unaligned.display().to_string(),
                "classes.dex".to_string(),
                "lib".to_string(),
            ],
            cwd: Some(apk),
            class_files: None,
        }));

        steps.push(Step::Run(RunStep {
            what: "align the APK",
            program: "zipalign".to_string(),
            args: vec![
                "-p".to_string(),
                "-f".to_string(),
                "4".to_string(),
                unaligned.display().to_string(),
                aligned.display().to_string(),
            ],
            cwd: None,
            class_files: None,
        }));

        if !self.keystore.is_file() {
            if let Some(parent) = self.keystore.parent() {
                steps.push(Step::MakeDir(parent.to_path_buf()));
            }
            steps.push(Step::Run(RunStep {
                what: "create the debug keystore",
                program: "keytool".to_string(),
                args: vec![
                    "-genkeypair".to_string(),
                    "-keystore".to_string(),
                    self.keystore.display().to_string(),
                    "-storepass".to_string(),
                    DEBUG_KEYSTORE_PASSWORD.to_string(),
                    "-keypass".to_string(),
                    DEBUG_KEYSTORE_PASSWORD.to_string(),
                    "-alias".to_string(),
                    "androiddebugkey".to_string(),
                    "-keyalg".to_string(),
                    "RSA".to_string(),
                    "-keysize".to_string(),
                    "2048".to_string(),
                    "-validity".to_string(),
                    "10000".to_string(),
                    "-dname".to_string(),
                    "CN=Android Debug,O=Android,C=US".to_string(),
                ],
                cwd: None,
                class_files: None,
            }));
        }

        if let Some(parent) = self.out.parent() {
            steps.push(Step::MakeDir(parent.to_path_buf()));
        }
        steps.push(Step::Run(RunStep {
            what: "sign the APK with the debug key",
            program: "apksigner".to_string(),
            args: vec![
                "sign".to_string(),
                "--ks".to_string(),
                self.keystore.display().to_string(),
                "--ks-pass".to_string(),
                format!("pass:{DEBUG_KEYSTORE_PASSWORD}"),
                "--out".to_string(),
                self.out.display().to_string(),
                aligned.display().to_string(),
            ],
            cwd: None,
            class_files: None,
        }));
        steps.push(Step::Run(RunStep {
            what: "verify the signature",
            program: "apksigner".to_string(),
            args: vec!["verify".to_string(), self.out.display().to_string()],
            cwd: None,
            class_files: None,
        }));

        steps
    }

    /// The line a successful run ends with.
    pub fn report(&self) -> Result<(), String> {
        let bytes = fs::metadata(&self.out)
            .map_err(|e| format!("read {}: {e}", self.out.display()))?
            .len();
        println!(
            "✓ {} — {} KB, {}, API {}–{}",
            self.out.display(),
            bytes / 1024,
            self.package,
            self.sdk.min_api,
            self.sdk.platform
        );
        Ok(())
    }

    fn work(&self) -> PathBuf {
        self.target_dir.join("android/work")
    }
}

impl Sdk {
    fn resolve(dry_run: bool) -> Result<Self, String> {
        let home = std::env::var_os("ANDROID_HOME")
            .or_else(|| std::env::var_os("ANDROID_SDK_ROOT"))
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let platform = env_or("ANDROID_PLATFORM", DEFAULT_PLATFORM);
        let min_api = env_or("ANDROID_MIN_API", DEFAULT_MIN_API);
        for (what, value) in [
            ("ANDROID_PLATFORM", &platform),
            ("ANDROID_MIN_API", &min_api),
        ] {
            if value.is_empty() || !value.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!("{what} must be a number, not `{value}`"));
            }
        }
        if !dry_run {
            let home = home.clone().ok_or_else(|| {
                "ANDROID_HOME is not set — run mq-apk inside the android devshell".to_string()
            })?;
            let jar = home
                .join(format!("platforms/android-{platform}"))
                .join("android.jar");
            if !jar.is_file() {
                return Err(format!(
                    "no android.jar for platform {platform} at {} — does the devshell pin it?",
                    jar.display()
                ));
            }
        }
        Ok(Self {
            home,
            platform,
            min_api,
        })
    }

    fn android_jar(&self) -> String {
        match &self.home {
            Some(home) => home
                .join(format!("platforms/android-{}/android.jar", self.platform))
                .display()
                .to_string(),
            None => format!(
                "${{ANDROID_HOME}}/platforms/android-{}/android.jar",
                self.platform
            ),
        }
    }

    fn describe(&self) -> String {
        let home = match &self.home {
            Some(home) => home.display().to_string(),
            None => "${ANDROID_HOME} (not set — dry run shows the symbolic path)".to_string(),
        };
        format!(
            "{home}, platform {}, min API {}",
            self.platform, self.min_api
        )
    }
}

fn env_or(name: &str, fallback: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fallback.to_string())
}

fn debug_keystore() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .ok_or("HOME is not set — the debug keystore lives at ~/.android/debug.keystore")?;
    Ok(PathBuf::from(home).join(".android/debug.keystore"))
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::RemoveDir(path) => write!(f, "rm -rf {}", path.display()),
            Step::MakeDir(path) => write!(f, "mkdir -p {}", path.display()),
            Step::Copy { from, to } => write!(f, "cp {} {}", from.display(), to.display()),
            Step::Write {
                path,
                contents,
                what,
            } => write!(
                f,
                "write {} ({what}, {} bytes)",
                path.display(),
                contents.len()
            ),
            Step::Run(run) => {
                if let Some(cwd) = &run.cwd {
                    write!(f, "cd {} && ", cwd.display())?;
                }
                write!(f, "{}", run.program)?;
                for arg in &run.args {
                    write!(f, " {}", quote(arg))?;
                }
                if let Some(dir) = &run.class_files {
                    write!(f, " {}/*.class", dir.display())?;
                }
                Ok(())
            }
        }
    }
}

/// Enough quoting for a readable dry run: paths with spaces stay one word.
fn quote(argument: &str) -> String {
    if argument.is_empty() || argument.contains(' ') {
        format!("'{argument}'")
    } else {
        argument.to_string()
    }
}

/// Run the plan. Every tool is checked first, so a missing one is named once
/// rather than failing halfway through with a half-built work directory.
pub fn execute(steps: &[Step]) -> Result<(), String> {
    preflight(steps)?;
    for step in steps {
        println!("→ {step}");
        match step {
            Step::RemoveDir(path) => {
                if path.exists() {
                    fs::remove_dir_all(path)
                        .map_err(|e| format!("remove {}: {e}", path.display()))?;
                }
            }
            Step::MakeDir(path) => {
                fs::create_dir_all(path).map_err(|e| format!("create {}: {e}", path.display()))?;
            }
            Step::Copy { from, to } => {
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("create {}: {e}", parent.display()))?;
                }
                fs::copy(from, to)
                    .map_err(|e| format!("copy {} → {}: {e}", from.display(), to.display()))?;
            }
            Step::Write { path, contents, .. } => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("create {}: {e}", parent.display()))?;
                }
                fs::write(path, contents).map_err(|e| format!("write {}: {e}", path.display()))?;
            }
            Step::Run(run) => run_step(run)?,
        }
    }
    Ok(())
}

fn preflight(steps: &[Step]) -> Result<(), String> {
    let mut missing: Vec<String> = Vec::new();
    for run in steps.iter().filter_map(|step| match step {
        Step::Run(run) => Some(run),
        _ => None,
    }) {
        let program = program_of(run);
        if !on_path(&program) && !missing.contains(&program) {
            missing.push(program);
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "not on PATH: {} — run mq-apk inside the android devshell",
            missing.join(", ")
        ))
    }
}

fn program_of(run: &RunStep) -> String {
    if run.program == "cargo" {
        std::env::var("CARGO").unwrap_or_else(|_| run.program.clone())
    } else {
        run.program.clone()
    }
}

fn on_path(program: &str) -> bool {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

fn run_step(run: &RunStep) -> Result<(), String> {
    let mut args = run.args.clone();
    if let Some(dir) = &run.class_files {
        let mut classes = Vec::new();
        classes_under(dir, &mut classes)?;
        classes.sort();
        if classes.is_empty() {
            return Err(format!(
                "{}: javac produced no .class files under {}",
                run.what,
                dir.display()
            ));
        }
        args.extend(classes.into_iter().map(|class| class.display().to_string()));
    }
    let program = program_of(run);
    let mut command = Process::new(&program);
    command.args(&args);
    if let Some(cwd) = &run.cwd {
        command.current_dir(cwd);
    }
    let status = command
        .status()
        .map_err(|e| format!("{}: run {program}: {e}", run.what))?;
    if !status.success() {
        return Err(format!("{}: {program} exited {status}", run.what));
    }
    Ok(())
}

fn classes_under(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read {}: {e}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            classes_under(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "class")
        {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(with_art: bool, keystore: &str) -> Build {
        Build {
            package: "au.defrag.example".into(),
            library: "example_mq".into(),
            krate: "example-mq".into(),
            cwd: PathBuf::from("/w"),
            target_dir: PathBuf::from("/w/target"),
            manifest_source: PathBuf::from("/w/example/android/AndroidManifest.xml"),
            manifest: "<manifest package=\"au.defrag.example\"/>\n".into(),
            activity: "package au.defrag.example;\n".into(),
            miniquad: "0.4.11".into(),
            miniquad_java: PathBuf::from("/src/miniquad-0.4.11/java"),
            art: with_art.then(|| Art {
                icon: PathBuf::from("/w/icon.png"),
                banner: PathBuf::from("/w/banner.png"),
            }),
            abis: Abi::ALL.to_vec(),
            out: PathBuf::from("/w/target/android/au.defrag.example.apk"),
            sdk: Sdk {
                home: Some(PathBuf::from("/sdk")),
                platform: "36".into(),
                min_api: "30".into(),
            },
            keystore: PathBuf::from(keystore),
        }
    }

    fn lines(build: &Build) -> Vec<String> {
        build.plan().iter().map(ToString::to_string).collect()
    }

    fn find(lines: &[String], needle: &str) -> usize {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("{needle} not in the plan:\n{}", lines.join("\n")))
    }

    #[test]
    fn both_abis_are_built_and_copied_into_the_apk() {
        let lines = lines(&build(false, "/nonexistent/debug.keystore"));
        for target in ["aarch64-linux-android", "armv7-linux-androideabi"] {
            assert!(
                lines.iter().any(|line| line.contains("cargo rustc -p example-mq")
                    && line.contains(target)),
                "no build for {target}"
            );
        }
        assert!(
            lines
                .iter()
                .any(|line| line.contains("work/apk/lib/arm64-v8a/libexample_mq.so"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("work/apk/lib/armeabi-v7a/libexample_mq.so"))
        );
    }

    #[test]
    fn the_pinned_sdk_versions_reach_d8_and_aapt2() {
        let lines = lines(&build(false, "/nonexistent/debug.keystore"));
        let d8 = find(&lines, "d8 --release");
        assert!(lines[d8].contains("--min-api 30"), "{}", lines[d8]);
        let link = find(&lines, "aapt2 link");
        assert!(
            lines[link].contains("--min-sdk-version 30"),
            "{}",
            lines[link]
        );
        assert!(
            lines[link].contains("--target-sdk-version 36"),
            "{}",
            lines[link]
        );
        assert!(lines[link].contains("/sdk/platforms/android-36/android.jar"));
    }

    #[test]
    fn the_rendered_files_are_what_gets_compiled_and_packaged() {
        let steps = build(false, "/nonexistent/debug.keystore").plan();
        let written: Vec<&Path> = steps
            .iter()
            .filter_map(|step| match step {
                Step::Write { path, contents, .. } if contents.contains("au.defrag.example") => {
                    Some(path.as_path())
                }
                _ => None,
            })
            .collect();
        assert!(written.contains(&Path::new(
            "/w/target/android/work/java/au/defrag/example/MainActivity.java"
        )));
        assert!(written.contains(&Path::new("/w/target/android/work/AndroidManifest.xml")));
        let javac = find(
            &lines(&build(false, "/nonexistent/debug.keystore")),
            "javac",
        );
        let plan = lines(&build(false, "/nonexistent/debug.keystore"));
        assert!(plan[javac].contains("java/au/defrag/example/MainActivity.java"));
        assert!(plan[javac].contains("java/quad_native/QuadNative.java"));
    }

    #[test]
    fn art_adds_the_resources_and_no_art_leaves_them_out() {
        let with = lines(&build(true, "/nonexistent/debug.keystore"));
        assert!(find(&with, "cp /w/icon.png") < find(&with, "aapt2 compile"));
        assert!(
            with.iter()
                .any(|line| line.contains("res/mipmap-xxxhdpi/ic_launcher.png"))
        );
        assert!(
            with[find(&with, "aapt2 link")].contains("work/res.zip"),
            "the link does not take the compiled resources"
        );

        let without = lines(&build(false, "/nonexistent/debug.keystore"));
        assert!(!without.iter().any(|line| line.contains("aapt2 compile")));
        assert!(!without[find(&without, "aapt2 link")].contains("res.zip"));
    }

    #[test]
    fn one_abi_builds_only_that_slice() {
        let mut build = build(false, "/nonexistent/debug.keystore");
        build.abis = vec![Abi::Arm64];
        let lines = lines(&build);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("--target aarch64-linux-android"))
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.contains("armv7-linux-androideabi"))
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("lib/arm64-v8a/libexample_mq.so"))
        );
        assert!(!lines.iter().any(|line| line.contains("lib/armeabi-v7a")));
    }

    #[test]
    fn an_abi_accepts_its_apk_or_rust_name() {
        for (value, expected) in [
            ("arm64-v8a", Abi::Arm64),
            ("aarch64-linux-android", Abi::Arm64),
            ("armeabi-v7a", Abi::Armv7),
            ("armv7-linux-androideabi", Abi::Armv7),
        ] {
            assert_eq!(Abi::parse(value).expect("parses"), expected);
        }
        assert!(Abi::parse("x86_64").is_err());
    }

    #[test]
    fn the_dry_run_shows_the_generated_class_files_as_a_glob() {
        let lines = lines(&build(false, "/nonexistent/debug.keystore"));
        let d8 = find(&lines, "d8 --release");
        assert!(lines[d8].contains("work/classes/*.class"), "{}", lines[d8]);
    }

    #[test]
    fn the_debug_keystore_is_created_only_when_it_is_missing() {
        let missing = lines(&build(false, "/nonexistent/debug.keystore"));
        assert!(
            missing
                .iter()
                .any(|line| line.contains("keytool -genkeypair"))
        );
        assert!(missing.iter().any(|line| line.contains("apksigner sign")
            && line.contains("--ks /nonexistent/debug.keystore")));

        let existing = lines(&build(
            false,
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
        ));
        assert!(!existing.iter().any(|line| line.contains("keytool")));
    }

    #[test]
    fn signing_comes_last_and_the_signature_is_verified() {
        let lines = lines(&build(false, "/nonexistent/debug.keystore"));
        let sign = find(&lines, "apksigner sign");
        let verify = find(&lines, "apksigner verify");
        let align = find(&lines, "zipalign -p -f 4");
        let zip = find(&lines, "zip -q -r");
        assert!(zip < align, "zip must precede zipalign");
        assert!(align < sign, "zipalign must precede signing");
        assert!(sign < verify, "verification must follow signing");
        assert_eq!(verify, lines.len() - 1, "verify is the last step");
    }

    #[test]
    fn a_dry_run_shows_a_symbolic_jar_when_no_sdk_is_present() {
        let mut build = build(false, "/nonexistent/debug.keystore");
        build.sdk.home = None;
        let lines = lines(&build);
        let link = find(&lines, "aapt2 link");
        assert!(
            lines[link].contains("${ANDROID_HOME}/platforms/android-36/android.jar"),
            "{}",
            lines[link]
        );
    }
}
