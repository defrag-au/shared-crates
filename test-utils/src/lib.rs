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
