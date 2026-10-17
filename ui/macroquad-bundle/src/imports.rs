//! What a wasm imports, and whether the page's scripts provide it.
//!
//! miniquad's gl.js does not refuse a wasm whose imports it cannot satisfy:
//! it stubs each missing one and logs "No <name> function in gl.js" when it
//! is called. A bundle missing a plugin therefore loads and fails quietly — a
//! silent game, deaf pads, a dead socket. [`unprovided`] makes that a build
//! failure.

/// One imported function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub module: String,
    pub name: String,
}

fn leb(data: &[u8], i: &mut usize) -> Result<u64, String> {
    let (mut value, mut shift) = (0u64, 0u32);
    loop {
        let byte = *data.get(*i).ok_or("truncated LEB128")?;
        *i += 1;
        value |= u64::from(byte & 0x7F) << shift;
        shift += 7;
        if byte < 0x80 {
            return Ok(value);
        }
        if shift > 63 {
            return Err("LEB128 too long".into());
        }
    }
}

fn name(data: &[u8], i: &mut usize) -> Result<String, String> {
    let n = leb(data, i)? as usize;
    let bytes = data.get(*i..*i + n).ok_or("truncated name")?;
    *i += n;
    String::from_utf8(bytes.to_vec()).map_err(|e| format!("import name: {e}"))
}

fn limits(data: &[u8], i: &mut usize) -> Result<(), String> {
    let flags = leb(data, i)?;
    leb(data, i)?;
    if flags & 1 != 0 {
        leb(data, i)?;
    }
    Ok(())
}

/// Every imported *function* in a wasm module, in order.
pub fn function_imports(wasm: &[u8]) -> Result<Vec<Import>, String> {
    if wasm.get(..4) != Some(b"\0asm") {
        return Err("not a wasm module".into());
    }
    let mut i = 8;
    let mut out = Vec::new();
    while i < wasm.len() {
        let section = wasm[i];
        i += 1;
        let size = leb(wasm, &mut i)? as usize;
        let end = i + size;
        if section != 2 {
            i = end;
            continue;
        }
        let count = leb(wasm, &mut i)?;
        for _ in 0..count {
            let module = name(wasm, &mut i)?;
            let field = name(wasm, &mut i)?;
            let kind = *wasm.get(i).ok_or("truncated import")?;
            i += 1;
            match kind {
                0 => {
                    leb(wasm, &mut i)?;
                    out.push(Import {
                        module,
                        name: field,
                    });
                }
                1 => {
                    i += 1; // reftype
                    limits(wasm, &mut i)?;
                }
                2 => limits(wasm, &mut i)?,
                3 => i += 2, // valtype + mutability
                k => return Err(format!("unknown import kind {k} for {module}.{field}")),
            }
        }
        break;
    }
    Ok(out)
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// `word` appears in `text` with no identifier character either side.
pub fn mentions(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
}

/// What the page cannot satisfy.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Unprovided {
    /// Imports from a module other than miniquad's `env` (wasm-bindgen's,
    /// typically): nothing in a miniquad page can provide them.
    pub foreign: Vec<String>,
    /// `env` functions no script mentions.
    pub missing: Vec<String>,
}

impl Unprovided {
    pub fn is_empty(&self) -> bool {
        self.foreign.is_empty() && self.missing.is_empty()
    }
}

/// Which imports no script provides. A name-in-text test, not a JS parser:
/// gl.js names its own imports, and each plugin assigns
/// `importObject.env.<name>`, so a provided function is always mentioned.
pub fn unprovided<'a>(
    imports: &[Import],
    scripts: impl IntoIterator<Item = &'a str>,
) -> Unprovided {
    let js: Vec<&str> = scripts.into_iter().collect();
    let mut out = Unprovided::default();
    for imp in imports {
        if imp.module != "env" {
            out.foreign.push(format!("{}.{}", imp.module, imp.name));
        } else if !js.iter().any(|s| mentions(s, &imp.name)) {
            out.missing.push(imp.name.clone());
        }
    }
    out.foreign.sort();
    out.foreign.dedup();
    out.missing.sort();
    out.missing.dedup();
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A minimal module: a type section, then an import section with the
    /// given (module, name) functions, a memory and a global.
    pub(crate) fn module(funcs: &[(&str, &str)]) -> Vec<u8> {
        let mut imports = Vec::new();
        let entries = funcs.len() + 2;
        imports.push(entries as u8);
        let push_name = |out: &mut Vec<u8>, s: &str| {
            out.push(s.len() as u8);
            out.extend_from_slice(s.as_bytes());
        };
        for (m, n) in funcs {
            push_name(&mut imports, m);
            push_name(&mut imports, n);
            imports.extend_from_slice(&[0, 0]); // function, type 0
        }
        push_name(&mut imports, "env");
        push_name(&mut imports, "memory");
        imports.extend_from_slice(&[2, 0, 17]); // memory, min 17
        push_name(&mut imports, "env");
        push_name(&mut imports, "g");
        imports.extend_from_slice(&[3, 0x7F, 0]); // global i32, const
        let mut wasm = b"\0asm\x01\0\0\0".to_vec();
        wasm.extend_from_slice(&[1, 4, 1, 0x60, 0, 0]); // type section: () -> ()
        wasm.push(2);
        wasm.push(imports.len() as u8);
        wasm.extend_from_slice(&imports);
        wasm
    }

    #[test]
    fn reads_function_imports_and_skips_the_rest() {
        let wasm = module(&[("env", "audio_init"), ("env", "glClear")]);
        let got = function_imports(&wasm).expect("parse");
        let names: Vec<&str> = got.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["audio_init", "glClear"]);
    }

    #[test]
    fn rejects_what_is_not_wasm() {
        assert!(function_imports(b"<html>").is_err());
    }

    #[test]
    fn a_name_counts_only_as_a_whole_word() {
        assert!(mentions(
            "importObject.env.audio_init = audio_init;",
            "audio_init"
        ));
        assert!(!mentions("function audio_init_later() {}", "audio_init"));
        assert!(!mentions("var $audio_init;", "audio_init"));
    }

    #[test]
    fn reports_missing_and_foreign_imports() {
        let imports = [
            Import {
                module: "env".into(),
                name: "glClear".into(),
            },
            Import {
                module: "env".into(),
                name: "audio_init".into(),
            },
            Import {
                module: "__wbindgen_placeholder__".into(),
                name: "__wbindgen_describe".into(),
            },
        ];
        let got = unprovided(&imports, ["function glClear() {}"]);
        assert_eq!(got.missing, ["audio_init"]);
        assert_eq!(
            got.foreign,
            ["__wbindgen_placeholder__.__wbindgen_describe"]
        );
    }
}
