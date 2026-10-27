use std::path::{Path, PathBuf};

use sceneanim::Clip;
use scenedrv::Driver;
use scenegraph::{Fonts, Geometry, Materials, Scene};
use sceneinput::Session;
use sceneworld::World;

use crate::act::Act;
use crate::clock::Clock;
use crate::error::Error;
use crate::patch::Delta;
use crate::replay::Replay;

const NOTO: &[u8] = scenegraph::NOTO_SANS_BOLD;

/// Closed filenames in an app folder.
pub const SCENE: &str = "scene.sg1";
pub const WORLD: &str = "world.sw1";
pub const MATERIALS: &str = "materials.mt1";
pub const CLIP: &str = "clip.an1";
pub const DRIVE: &str = "drive.drv1";
pub const SESSION: &str = "session.in1";
pub const AFTER: &str = "after.sg1";
pub const ACT: &str = "act.act1";
pub const DELTA: &str = "delta.sgd1";
pub const CLOCK: &str = "clock.tk1";
pub const REPLAY: &str = "replay.rp1";
/// Directory of binary glTF models, one per file, keyed by file stem.
pub const MODELS: &str = "models";
/// Directory of PNG / JPEG images, keyed by file stem (`m=STEM`).
pub const IMAGES: &str = "images";

/// One dump-folder app. Rest is `scene.sg1` or `world.sw1` emit.
pub struct App {
    pub dir: PathBuf,
    pub scene: Scene,
    pub materials: Materials,
    pub clip: Option<Clip>,
    pub driver: Option<Driver>,
    pub session: Session,
    pub act: Option<Act>,
    pub delta: Option<Delta>,
    pub world: Option<World>,
    /// `after.sg1`, parsed. What one ply or `clock.n` ticks must write.
    pub after: Option<Scene>,
    pub replay: Option<Replay>,
    pub geom: Geometry,
    /// Pixels the folder brings: its models' textures (`KEY/NAME`).
    pub images: scenegraph::Images,
    pub fonts: Fonts,
    pub clock: Clock,
    /// `clock.tk1` was in the folder — the pane steps the playhead every frame.
    pub clock_live: bool,
    files: Vec<String>,
}

/// The stock face (`ibm`, NotoSans-Bold) every app can name.
pub fn stock_fonts() -> Result<Fonts, Error> {
    let mut fonts = Fonts::new();
    fonts
        .insert("ibm", NOTO, Vec::new())
        .map_err(|e| Error::Scene {
            path: "ibm".into(),
            source: e,
        })?;
    Ok(fonts)
}

impl App {
    /// Read a folder from disk, with every `models/NAME.glb` as model `NAME`
    /// and every `images/NAME.png|jpg` as image `NAME`.
    pub fn load(dir: impl AsRef<Path>) -> Result<Self, Error> {
        let dir = dir.as_ref().to_path_buf();
        let root = dir.clone();
        let mut app = Self::from_source(dir.clone(), move |name| {
            std::fs::read_to_string(root.join(name)).ok()
        })?;
        for path in files_in(&dir.join(MODELS), &["glb"]) {
            let Some(key) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let bytes =
                std::fs::read(&path).map_err(|e| Error::io(format!("{}: {e}", path.display())))?;
            app.add_model(key, &bytes)?;
        }
        for path in files_in(&dir.join(IMAGES), &["png", "jpg", "jpeg"]) {
            let Some(key) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let bytes =
                std::fs::read(&path).map_err(|e| Error::io(format!("{}: {e}", path.display())))?;
            app.add_image(key, &bytes)?;
        }
        Ok(app)
    }

    /// Decode a PNG or JPEG as image `key` (`m=KEY`): what [`App::load`]
    /// does for each `images/KEY.png`, so the checks can read its pixels.
    pub fn add_image(&mut self, key: &str, bytes: &[u8]) -> Result<(), Error> {
        let path = format!("{IMAGES}/{key}");
        let rgba = image::load_from_memory(bytes)
            .map_err(|e| Error::io(format!("{path}: {e}")))?
            .to_rgba8();
        let (w, h) = rgba.dimensions();
        self.images
            .insert(key, w, h, rgba.into_raw())
            .map_err(|e| Error::Scene {
                path: path.clone(),
                source: e,
            })?;
        self.files.push(path);
        Ok(())
    }

    /// Intern a binary glTF as model `key`: a `Mesh Geom g=KEY` node draws
    /// it whole and picks as that node. Its textures join [`App::images`]
    /// as `KEY/NAME`. What [`App::load`] does for each `models/KEY.glb`; a
    /// client without a filesystem calls it with embedded bytes.
    pub fn add_model(&mut self, key: &str, bytes: &[u8]) -> Result<(), Error> {
        let path = format!("{MODELS}/{key}.glb");
        let scene_err = |e| Error::Scene {
            path: path.clone(),
            source: e,
        };
        let model = scenegraph::model_from_gltf(key, bytes).map_err(scene_err)?;
        let images = self.geom.insert_imported(key, model).map_err(scene_err)?;
        self.images.absorb(images);
        self.files.push(path);
        Ok(())
    }

    /// Read a folder through `read(file_name)`: `None` is an absent file.
    /// No I/O of its own, so a client can embed its folders (`include_str!`)
    /// where there is no filesystem. `dir` only labels the app.
    pub fn from_source(
        dir: impl Into<PathBuf>,
        read: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, Error> {
        let dir = dir.into();
        let mut files = Vec::new();
        let mut take = |name: &str| {
            let src = read(name);
            if src.is_some() {
                files.push(name.to_string());
            }
            src
        };

        let world = take(WORLD)
            .map(|src| {
                World::from_dump(&src).map_err(|e| Error::World {
                    path: WORLD.to_string(),
                    source: e,
                })
            })
            .transpose()?;
        let emitted = world
            .as_ref()
            .map(|w| w.emit())
            .transpose()
            .map_err(|e| Error::World {
                path: WORLD.to_string(),
                source: e,
            })?;
        let scene = match take(SCENE) {
            Some(src) => {
                let loaded = parse_scene(SCENE, &src)?;
                if let Some(emitted) = &emitted {
                    if loaded.dump() != emitted.dump() {
                        return Err(Error::scenes(
                            "scene.sg1 != world.sw1 emit (want is the emit)",
                            &emitted.dump(),
                            &loaded.dump(),
                        ));
                    }
                }
                loaded
            }
            None => emitted.ok_or(Error::MissingRest)?,
        };

        let materials = match take(MATERIALS) {
            Some(src) => Materials::from_dump(&src).map_err(|e| Error::Scene {
                path: MATERIALS.to_string(),
                source: e,
            })?,
            None => Materials::new(),
        };
        let clip = take(CLIP)
            .map(|src| {
                Clip::from_dump(&src).map_err(|e| Error::Anim {
                    path: CLIP.to_string(),
                    source: e,
                })
            })
            .transpose()?;
        let driver = take(DRIVE)
            .map(|src| {
                Driver::from_dump(&src).map_err(|e| Error::Drive {
                    path: DRIVE.to_string(),
                    source: e,
                })
            })
            .transpose()?;
        let session = match take(SESSION) {
            Some(src) => Session::from_dump(&src).map_err(|e| Error::Input {
                path: SESSION.to_string(),
                source: e,
            })?,
            None => Session::new(),
        };
        let act = take(ACT).map(|src| Act::from_dump(&src)).transpose()?;
        let after = take(AFTER)
            .map(|src| parse_scene(AFTER, &src))
            .transpose()?;
        let delta = take(DELTA).map(|src| Delta::from_dump(&src)).transpose()?;
        let (clock, clock_live) = match take(CLOCK) {
            Some(src) => (Clock::from_dump(&src)?, true),
            None => (Clock::default(), false),
        };
        let replay = take(REPLAY)
            .map(|src| Replay::from_dump(&src))
            .transpose()?;

        let mut geom = Geometry::new();
        geom.insert_tet("tet").map_err(|e| Error::Scene {
            path: "tet".into(),
            source: e,
        })?;
        let fonts = stock_fonts()?;

        Ok(Self {
            dir,
            scene,
            materials,
            clip,
            driver,
            session,
            act,
            delta,
            world,
            after,
            replay,
            geom,
            images: scenegraph::Images::new(),
            fonts,
            clock,
            clock_live,
            files,
        })
    }

    /// Files present, in load order.
    pub fn files(&self) -> &[String] {
        &self.files
    }
}

/// Files directly in `dir` with one of `exts` (any case), sorted; none if
/// the directory is absent.
fn files_in(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
        })
        .collect();
    out.sort();
    out
}

fn parse_scene(path: &str, src: &str) -> Result<Scene, Error> {
    Scene::from_dump(src).map_err(|e| Error::Scene {
        path: path.to_string(),
        source: e,
    })
}
