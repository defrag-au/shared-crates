/// Load a test-case file from `resources/test/` and return its contents as a
/// `&'static str`.
///
/// # Why it leaks
///
/// The contents are read at runtime and then **deliberately leaked**, which is
/// what makes the `&'static str` honest. Callers pass the result straight to
/// `serde_json::from_str` in a `match` scrutinee, and any borrowed alternative
/// would be a reference into a temporary.
///
/// It used to be exactly that — the tail was `&buff.to_string()`, a reference
/// into a clone created in the block's own tail expression. Edition 2021
/// extended that temporary to the enclosing statement and it happened to work;
/// **edition 2024 drops it at the end of the block**, so all twenty call sites
/// broke at once. Leaking is the fix that keeps the signature the doc comment
/// always claimed.
///
/// The cost is bounded by the number of test cases and reclaimed when the test
/// binary exits. Do not use this outside tests.
///
/// (The previous doc comment also claimed this embedded the file with
/// `include_str!`. It never did.)
///
/// For a `String` you own, use [`load_test_resource!`].
#[macro_export]
macro_rules! test_case {
    ($fname:expr) => {{
        let filename = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/test/", $fname);
        let mut file = std::fs::File::open(filename).unwrap();
        let mut buff = String::new();
        use ::std::io::Read;
        file.read_to_string(&mut buff).unwrap();
        &*::std::boxed::Box::leak(buff.into_boxed_str())
    }};
}

/// Load a test resource file at runtime and return its contents as a String.
/// This macro is useful for large test files that should not be embedded at compile time
/// to avoid compilation issues in CI environments.
///
/// # Example
/// ```no_run
/// use test_utils::load_test_resource;
/// let json_data = load_test_resource!("large_block.json");
/// ```
#[macro_export]
macro_rules! load_test_resource {
    ($fname:expr) => {{
        use std::path::PathBuf;
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("resources/test/");
        path.push($fname);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("Failed to read test file: {}", path.display()))
    }};
}

pub fn init_test_tracing() {
    // Use a simple formatting subscriber for local dev/test logs.
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .compact()
        .finish();

    // Set as the default global subscriber (only once!)
    let _ = tracing::subscriber::set_global_default(subscriber);
}
