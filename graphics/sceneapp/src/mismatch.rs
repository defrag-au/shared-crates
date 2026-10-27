//! Say *how* two dumps differ, so a mismatch is fixable from its message.
//! A scene is compared by node name (the dump's identity); anything else by
//! line.

use std::collections::{BTreeSet, HashMap};

/// Per-node difference of two `sg1` dumps: each node whose line differs, is
/// missing, or is extra, as `want:` / `got:` pairs in `got`'s order, then
/// `want`'s leftovers. Same nodes and lines in another order says so.
pub fn scenes(want: &str, got: &str) -> String {
    let index = |d: &str| -> Vec<(String, String)> {
        d.lines()
            .skip(1)
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .filter_map(|l| Some((l.split_whitespace().next()?.to_string(), l.to_string())))
            .collect()
    };
    let w = index(want);
    let g = index(got);
    let wmap: HashMap<&str, &str> = w.iter().map(|(n, l)| (n.as_str(), l.as_str())).collect();
    let gmap: HashMap<&str, &str> = g.iter().map(|(n, l)| (n.as_str(), l.as_str())).collect();
    let mut out = String::new();
    for (name, gl) in &g {
        match wmap.get(name.as_str()) {
            Some(wl) if wl == gl => {}
            Some(wl) => out.push_str(&format!("want: {wl}\ngot:  {gl}\n")),
            None => out.push_str(&format!("want: (no {name})\ngot:  {gl}\n")),
        }
    }
    for (name, wl) in &w {
        if !gmap.contains_key(name.as_str()) {
            out.push_str(&format!("want: {wl}\ngot:  (no {name})\n"));
        }
    }
    if out.is_empty() && want != got {
        let order = |v: &[(String, String)]| {
            v.iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        out.push_str(&format!(
            "same nodes, another order\nwant: {}\ngot:  {}\n",
            order(&w),
            order(&g)
        ));
    }
    out
}

/// Line difference of two dumps (a delta, an act): what only `want` has,
/// then what only `got` has. Order alone differing says so.
pub fn lines(want: &str, got: &str) -> String {
    let w: BTreeSet<&str> = want.lines().collect();
    let g: BTreeSet<&str> = got.lines().collect();
    let mut out = String::new();
    for l in want.lines().filter(|l| !g.contains(l)) {
        out.push_str(&format!("want: {l}\n"));
    }
    for l in got.lines().filter(|l| !w.contains(l)) {
        out.push_str(&format!("got:  {l}\n"));
    }
    if out.is_empty() && want != got {
        out.push_str("same lines, another order\nwant:\n");
        out.push_str(want);
        out.push_str("got:\n");
        out.push_str(got);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_node_shows_both_lines() {
        let d = scenes("sg1\na Group\nb Quad 1x1\n", "sg1\na Group\nb Quad 2x1\n");
        assert_eq!(d, "want: b Quad 1x1\ngot:  b Quad 2x1\n");
    }

    #[test]
    fn missing_and_extra_nodes_are_named() {
        let d = scenes("sg1\na Group\nb Quad 1x1\n", "sg1\na Group\nc Quad 1x1\n");
        assert_eq!(
            d,
            "want: (no c)\ngot:  c Quad 1x1\nwant: b Quad 1x1\ngot:  (no b)\n"
        );
    }

    #[test]
    fn order_alone_is_said() {
        let d = scenes("sg1\na Group\nb Group\n", "sg1\nb Group\na Group\n");
        assert!(d.starts_with("same nodes, another order\n"), "{d}");
    }

    #[test]
    fn delta_lines_split_by_side() {
        let d = lines("sgd1\n~ a x\n+ b y\n", "sgd1\n+ b y\n~ a z\n");
        assert_eq!(d, "want: ~ a x\ngot:  ~ a z\n");
        let o = lines("sgd1\n~ a x\n+ b y\n", "sgd1\n+ b y\n~ a x\n");
        assert!(o.starts_with("same lines, another order\n"), "{o}");
    }
}
