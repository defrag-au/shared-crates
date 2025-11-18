/// Load a test case file from the resources/test directory and return its contents as a &'static str.
/// This macro embeds the file contents at compile time using include_str!.
///
/// For large files that should be loaded at runtime to avoid compilation issues,
/// use `load_test_resource!` instead.
#[macro_export]
macro_rules! test_case {
    ($fname:expr) => {{
        let filename = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/test/", $fname);
        let mut file = std::fs::File::open(filename).unwrap();
        let mut buff = String::new();
        use ::std::io::Read;
        file.read_to_string(&mut buff).unwrap();
        &buff.to_string()
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
