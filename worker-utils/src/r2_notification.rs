use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BucketNotification {
    pub account: String,
    pub action: String,
    pub bucket: String,
    pub event_time: String,
    pub object: BucketNotificationObject,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BucketNotificationObject {
    pub e_tag: String,
    pub key: String,
    pub size: u64,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::assertions_on_constants)]

    use super::*;
    use std::fs::File;
    use std::io::Read;

    macro_rules! test_case {
        ($fname:expr) => {{
            let filename = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/test/", $fname);
            let mut file = File::open(filename).unwrap();
            let mut buff = String::new();
            file.read_to_string(&mut buff).unwrap();

            println!("buff: {}", &buff.to_string());
            &buff.to_string()
        }};
    }

    #[test]
    fn test_deserialize() {
        match serde_json::from_str::<BucketNotification>(test_case!("sample_put.json")) {
            Ok(notification) => {
                assert_eq!(notification.account, "32b222df0246cc40477b703275f6ff3f");
                assert_eq!(notification.action, "PutObject");
                assert_eq!(notification.bucket, "cnft-dev-ipfs-mirror");
                assert_eq!(notification.event_time, "2025-01-15T08:50:43.481Z");
                assert_eq!(
                    notification.object.e_tag,
                    "f1d2b08c1d89415264d1b0e9a7238eb7"
                );
                assert_eq!(
                    notification.object.key,
                    "b3dab69f7e6100849434fb1781e34bd12a916557f6231b8d2629b6f6/50697261746531.png"
                );
                assert_eq!(notification.object.size, 3322516);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }
}
