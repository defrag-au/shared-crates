//! Submit census: what a backend hands its GPU, as triangles per texture.
//! Backends build it from their own submission so two backends can be
//! compared without a window or an adapter. Not part of `sg1`.

use std::collections::BTreeMap;

/// Triangles grouped by (texture key, MSDF) plus totals.
///
/// ```text
/// submit-census
/// draw key=- sdf=0 tris=12
/// draw key=ibm sdf=1 tris=36
/// total tris=48 quads=2 meshes=1 labels=1
/// ```
///
/// `key=-` is untextured (white × colour). Order is sorted by key, then sdf.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubmitCensus {
    groups: BTreeMap<(String, bool), u32>,
    pub quads: u32,
    pub meshes: u32,
    pub labels: u32,
}

impl SubmitCensus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Count `indices / 3` triangles drawn with `key` (None = untextured).
    pub fn add(&mut self, key: Option<&str>, sdf: bool, indices: u32) {
        if indices == 0 {
            return;
        }
        let k = key.unwrap_or("-").to_string();
        *self.groups.entry((k, sdf)).or_insert(0) += indices / 3;
    }

    pub fn tris(&self) -> u32 {
        self.groups.values().sum()
    }

    pub fn dump(&self) -> String {
        let mut out = String::from("submit-census\n");
        for ((key, sdf), tris) in &self.groups {
            let sdf = u8::from(*sdf);
            out.push_str(&format!("draw key={key} sdf={sdf} tris={tris}\n"));
        }
        out.push_str(&format!(
            "total tris={} quads={} meshes={} labels={}\n",
            self.tris(),
            self.quads,
            self.meshes,
            self.labels
        ));
        out
    }
}
