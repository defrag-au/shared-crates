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

pub fn init_test_tracing() {
    // Use a simple formatting subscriber for local dev/test logs.
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .compact()
        .finish();

    // Set as the default global subscriber (only once!)
    let _ = tracing::subscriber::set_global_default(subscriber);
}
