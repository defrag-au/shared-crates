use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("duplicate node name '{0}'")]
    DuplicateName(String),
    #[error("unknown node '{0}'")]
    UnknownNode(String),
    #[error("unknown parent '{0}'")]
    UnknownParent(String),
    #[error("parent cycle involving '{0}'")]
    Cycle(String),
    #[error("unknown kind '{0}'")]
    UnknownKind(String),
    #[error("unknown key '{0}'")]
    UnknownKey(String),
    #[error("dump parse: {0}")]
    Parse(String),
    #[error("{0:?} does not take {1}")]
    KindKey(crate::kind::KindOf, &'static str),
    #[error("Quad needs WxH")]
    QuadNeedsSize,
    #[error("Mesh needs Box|Plane|Geom|Slab|Sphere")]
    MeshNeedsPrim,
    #[error("Slab needs WxH")]
    SlabNeedsSize,
    #[error("Geom needs g=")]
    MeshNeedsGeom,
    #[error("unknown prim '{0}'")]
    UnknownPrim(String),
    #[error("Light needs Dir|Point")]
    LightNeedsLamp,
    #[error("unknown lamp '{0}'")]
    UnknownLamp(String),
    #[error("multiple active cameras")]
    MultipleActive,
    #[error("bad image '{0}'")]
    BadImage(String),
    #[error("unknown geom '{0}'")]
    UnknownGeom(String),
    #[error("{0}: pose names clip '{1}', which its model does not have (the census lists each model's clips)")]
    UnknownPose(String, String),
    #[error("unknown font '{0}'")]
    UnknownFont(String),
    #[error("Label needs font=")]
    LabelNeedsFont,
    #[error("Label needs size=")]
    LabelNeedsSize,
    #[error("Label needs a quoted string")]
    LabelNeedsText,
    #[error("font atlas full for '{0}'")]
    FontAtlasFull(String),
    #[error("bad font '{0}'")]
    BadFont(String),
    #[error("bad geom '{0}'")]
    BadGeom(String),
    #[error("unknown shade '{0}'")]
    UnknownShade(String),
    #[error("unknown material '{0}'")]
    UnknownMaterial(String),
    #[error("duplicate material '{0}'")]
    DuplicateMaterial(String),
    #[error("bad material '{0}'")]
    BadMaterial(String),
    #[error("mat= cannot mix with c, m, mb, or sh")]
    MatInline,
    #[error("no camera in scene")]
    NoCamera,
    #[error("gpu: {0}")]
    Gpu(String),
    #[error("gltf: {0}")]
    Gltf(String),
}
