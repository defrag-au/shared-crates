#[cfg(test)]
mod tests {
    #![allow(clippy::assertions_on_constants)]

    use crate::*;
    use cardano_assets::IntoTraits;
    use std::collections::HashMap;

    macro_rules! test_case {
        ($fname:expr) => {{
            use std::fs::File;
            use std::io::Read;

            let filename = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/test/", $fname);
            let mut file = File::open(filename).unwrap();
            let mut buff = String::new();
            file.read_to_string(&mut buff).unwrap();

            strip_control_chars(&buff)
        }};
    }

    #[test]
    fn test_deserialize() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("asset_info.json")) {
            Ok(pirate) => {
                let test_traits = HashMap::from([
                    ("Eyes", "Patch"),
                    ("Nose", "Blocky"),
                    ("Rank", "Navigator"),
                    ("Skin", "Pale"),
                    ("Mouth", "Dark Curl"),
                    ("Weapon", "Blackbeard's Wrath"),
                    ("Clothes", "Violet Buccaneer"),
                    ("Headwear", "Ethereal Hat"),
                    ("Background", "Emerald Isle"),
                ])
                .into_traits();
                let asset: Asset = pirate.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Pirate #1");
                assert_eq!(
                    asset.image,
                    "ipfs://QmWhLmt9BXdxdK6VeaZHWyLkeHfszwyRqgbXeiUyPimMaR"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_snekkie() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("snekkie.json")) {
            Ok(snekkie) => {
                let test_traits = HashMap::from([
                    ("Background", "Apricot"),
                    ("Dome", "Snekkie Troop"),
                    ("Eyes", "Laser Eyes Blue"),
                    ("Face", "Intense Beard"),
                    ("Skin", "Black"),
                    ("Style", "Clean"),
                    ("Type", "Floor"),
                ])
                .into_traits();
                let asset: Asset = snekkie.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Snekkie #0299");
                assert_eq!(
                    asset.image,
                    "ipfs://Qmd72yzZke1yJFRJuJp7R1coEjq12PEQbb2jMJBqqV9Ft5"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_jellycube() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("jellycube.json")) {
            Ok(jellycube) => {
                let test_traits = HashMap::from([
                    ("Backgrounds", "Baby Pink"),
                    ("Block", "Jellyiet"),
                    ("Charisma", "2"),
                    ("Class", "Fighter"),
                    ("Combat Score", "11"),
                    ("Constitution", "1"),
                    ("Dexterity", "5"),
                    ("Face", "Supportive"),
                    ("Filler", "Dragon Cubes"),
                    ("Intellect", "1"),
                    ("LeftTop", "Archery"),
                    ("Main", "Disco"),
                    ("Points", "1"),
                    ("RightTop", "Trickster"),
                    ("Secondary", "Tacos"),
                    ("Strength", "1"),
                    ("Wisdom", "1"),
                ])
                .into_traits();
                let asset: Asset = jellycube.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Rathyron of Jellyiet");
                assert_eq!(
                    asset.image,
                    "ipfs://QmP24on5FgEBLJ5dE3pUSMdyqF95rM9irLvHtpHhE2eoFS"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_aquafarmer() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("aquafarmer.json")) {
            Ok(nft) => {
                let test_traits = HashMap::from([
                    ("Arm mechanics", "Purple mechanics"),
                    ("Background", "Wheat fields"),
                    ("Background accessories", "Windmill"),
                    ("Farmer body color", "Black gold"),
                    ("Farmer clothing", "None"),
                    ("Farmer head", "Cylinder head 1 eye"),
                    ("Hat", "None"),
                    ("Left hand tool", "Broom"),
                    ("Right hand tool", "Flower"),
                    ("Tier", "Rare"),
                ])
                .into_traits();
                let asset: Asset = nft.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Aquafarmer #453");
                assert_eq!(
                    asset.image,
                    "ipfs://QmcJoW5VxNgRxBXSCwYCfpNP8JQ8inU3jWkUqkmSfxS3be"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_jrnyclub() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("jrnyclub.json")) {
            Ok(nft) => {
                let test_traits = HashMap::from([
                    ("Background", "Matte Gray 1"),
                    ("Element", "Air"),
                    ("Eyes", "Determined Eye"),
                    ("Head", "Windbreak Technician Head"),
                    ("Implants", "Implant Design 5"),
                    ("Mouth", "Happy Mouth"),
                    ("Outfit", "Corpo Nepo Outfit"),
                    ("Skill", "None"),
                    ("tokenId", "4471"), // Extra field included as trait
                ])
                .into_traits();
                let asset: Asset = nft.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "#4471");
                assert_eq!(
                    asset.image,
                    "ipfs://QmdJZi8J4hASwFj59Dm396NtWoBFcsX2tJwXfruooo5Xas"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_mallard() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("mallard.json")) {
            Ok(snekkie) => {
                let test_traits = HashMap::from([
                    ("Accessories", "None"),
                    ("Head", "None"),
                    ("Mask", "None"),
                    ("Beak", "Plain"),
                    ("Eyewear", "None"),
                    ("Eyes", "Feline"),
                    ("Face", "None"),
                    ("Neckwear", "None"),
                    ("Clothes", "Sailor Shirt"),
                    ("Skin", "None"),
                    ("Feathers", "Skeleton"),
                    ("Back", "None"),
                    ("Background", "Black"),
                    ("School of Thought", "Magicka"),
                ])
                .into_traits();
                let asset: Asset = snekkie.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "The Mallard Order #4835");
                assert_eq!(
                    asset.image,
                    "ipfs://QmPEysw5BQGp9QaMSYQn8ruoQhwaNNPzXkbGeV5x1Lc9v4"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_havoc() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("havoc_worlds.json")) {
            Ok(cnft) => {
                let test_traits = HashMap::from([
                    ("Background", "Electric White"),
                    ("Xeno Head Item", "Bandage Orange"),
                    ("Xeno Weapon", "Plasma Rifle White"),
                    ("Xeno Bonus Item", "None"),
                    ("Xeno Base", "Base 1"),
                    ("Xeno Clothes", "Advance Armour Yellow"),
                    ("Xeno Marking", "Scar 1"),
                    ("Xeno Piercing", "Spike"),
                ])
                .into_traits();
                let asset: Asset = cnft.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Havoc Worlds #5972");
                assert_eq!(
                    asset.image,
                    "ipfs://QmZaGQemF5noCZZRda6qX4sf2n9cwvPg4H2sRb2CXeGX4k/5972.png"
                );
                assert_eq!(asset.media_type, Some("image/png".to_string()));
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_havoc_gif() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("havoc_worlds_gif.json")) {
            Ok(cnft) => {
                let test_traits = HashMap::from([
                    ("Background", "Tech Green"),
                    ("Male Hair", "Faux Hawk Teal"),
                    ("Male Face", "Bliss"),
                    ("Human Piercing", "Spike"),
                    ("Human Bonus Item", "None"),
                    ("Male Clothes", "Advance Vest Navy"),
                    ("Human Head Item", "Sunglasses Black"),
                    ("Human Weapon", "Shotgun 1"),
                    ("Male Marking", "Brushed Maroon"),
                    ("Male Base", "Male 1"),
                ])
                .into_traits();
                let asset: Asset = cnft.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Havoc Worlds #4329");
                assert_eq!(asset.media_type, Some("image/gif".to_string()));
                assert_eq!(
                    asset.image,
                    "ipfs://QmYtDxto2fvtNXeyjiwyAwGgnFkKvH4gJ12dg5jgQo7BAU"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_lycan() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("unbothered_lycan.json")) {
            Ok(cnft) => {
                let test_traits = HashMap::from([
                    ("Eyes", "Wink"),
                    ("Fur", "Dalmatian"),
                    ("Corps", "Champion"),
                    ("Head", "None"),
                    ("Mouth", "Cigarette"),
                    ("Clothing", "Pioneer Warrior"),
                    ("Earring", "None"),
                    ("Background", "Red"),
                ])
                .into_traits();
                let asset: Asset = cnft.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Unbothered Lycan #2331");
                assert_eq!(
                    asset.image,
                    "ipfs://QmRckaiw6QgicKWfz8xvWptFxbDNDBsFDDe9atc3m7odoN"
                );
                assert_eq!(asset.media_type, Some("image/png".to_string()));
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_working_dead() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("workingdead.json")) {
            Ok(snekkie) => {
                let test_traits = HashMap::from([
                    ("Company", "Ever After Enterprises"),
                    ("Role", "Intern"),
                    ("background", "Electric"),
                    ("body", "Soccer Uniform"),
                    ("color", "White"),
                    ("eyes", "Peace"),
                    ("flame", "Pink"),
                    ("head", "Bonehawk"),
                    ("skull", "Missing Tooth"),
                    ("utility", "Thumbs Up"),
                ])
                .into_traits();
                let asset: Asset = snekkie.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "The Working Dead #3636");
                assert_eq!(
                    asset.image,
                    "ipfs://QmaZmAzAB1mzMkkPDLpWt9QCgWvLtAYrK5L4wyzV2Chzd2"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_adagods() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("adagods.json")) {
            Ok(snekkie) => {
                let test_traits = HashMap::from([
                    ("Eyes", "Sunglasses Gold"),
                    ("Hats", "Hawk Hair Grey"),
                    ("Skins", "Black"),
                    ("Mouths", "Beard with Cigar Grey"),
                    ("Clothes", "Puffer Vest White"),
                    ("Backgrounds", "Basic 10"),
                ])
                .into_traits();
                let asset: Asset = snekkie.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "AdaGod #0924");
                assert_eq!(
                    asset.image,
                    "ipfs://Qmb7J1pnwQJnXhPy72QXsaeE6uFNk6FJ9xcc6QizSjLTcv"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_toolheads() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("toolhead.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("background", "Cybernetic Sea"),
                    ("body", "Rough"),
                    ("accessory", "Recon Pack"),
                    ("outfit", "Sensory Shirt"),
                    ("strap", "Grenade Girdle"),
                    ("head", "Cyberweave, Necromancer Mask"),
                    ("role", "Enforcer"),
                ])
                .into_traits();
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Toolhead #3374");
                assert_eq!(
                    asset.image,
                    "ipfs://QmZ3JHAhH9B7TFLcWsAhaJmFRjX56j7GC6Cn8wkh7xja6S"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                panic!("failed decoding: {err:?}");
            }
        }
    }

    #[test]
    fn test_deserialize_viperion() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("viperion.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("Eyes", "Robot Pirate"),
                    ("Face", "Scroll"),
                    ("Head", "Majin Blue"),
                    ("Skin", "Cosmic Blaze"),
                    ("Type", "Army"),
                    ("Clothes", "Mech Commander"),
                    ("Background", "Royal Fang"),
                ])
                .into_traits();
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Chaos Mamba");
                assert_eq!(
                    asset.image,
                    "ipfs://Qmebkd8ULHggAaGf8LjmLit7VskvdhEu67MG3z5jHvxoFP"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                panic!("failed decoding: {err:?}");
            }
        }
    }

    #[test]
    fn test_deserialize_ug() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("ug.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("Background", "Greenicles"),
                    ("Skin", "Peach"),
                    ("Outfit", "Stockbroker"),
                    ("Hats", "Spiky Blue Hair"),
                    ("Mouth", "Safety Pin"),
                    ("Earring", "GM"), // Note: actual data has trailing space but into_traits() trims it
                    ("Eyenose", "Lowbrow"),
                ])
                .into_traits();

                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Uggler Glimmersnap");
                assert_eq!(
                    asset.image,
                    "ipfs://QmU4ZJeFptaphEsHCK4tia4wc7swjwo4gh38jwK7rmYgtc"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_ug_554738363536() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("ug_554738363536.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("Background", "Blurple"),
                    ("Skin", "Green Stripes"),
                    ("Outfit", "Noodles"),
                    ("Hats", "Lethal Electric Current"),
                    ("Mouth", "Stumped"),
                    ("Eyenose", "Four-eyed"),
                ])
                .into_traits();

                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Ugul Sloshbloop");
                assert_eq!(
                    asset.image,
                    "ipfs://QmZSCEY8gR1wtUKmW9rqG7yWDxbnYccRboc4rmU2zbdx8g"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_ug_554738333630() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("ug_554738333630.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("Background", "First Rays"),
                    ("Skin", "Blue"),
                    ("Outfit", "Hammock"),
                    ("Hats", "Pink Spidery Hair with Surprise"),
                    ("Mouth", "Gremlin"),
                    ("Eyenose", "Gnunkle"),
                ])
                .into_traits();

                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Ugster Whirryplunk");
                assert_eq!(
                    asset.image,
                    "ipfs://QmVKonAkP9ikaXCtZXRjPeXeotKgdqj6VrqprfvwaSBHEs"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_ug_5547546f7937373939() {
        // NOTE: from looking at this asset it looks like it was minted, and then burnt (and then reminted)
        // which makes it a good example of why Maestro doesn't have any metadata on the asset so it's correct
        // that the asset can't be deserialized
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("ug_5547546f7937373939.json")) {
            Ok(deserialized) => {
                let asset = Asset::try_from(deserialized.data.asset_standards);
                match asset {
                    Ok(_) => panic!("invalid asset deserialized - this should NOT happen"),
                    Err(MaestroError::NoMetadata) => {}
                    Err(_) => panic!("asset deserialized failed, but incorrect error raised"),
                }
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_wavy_dupe() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("wavy_dupe.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("Background", "Yellow"),
                    ("Back", "Ganja Sack"),
                    ("Body", "Brown"),
                    ("Outfit", "Polo Stripey"),
                    ("Neck", "None"),
                    ("Mask", "None"),
                    ("Ears", "Elf"),
                    ("Earrings", "Skill"),
                    ("Accessory", "Copium"),
                    ("Mouth", "Grin"),
                    ("Eyes", "Small"),
                    ("Nose", "Smile"),
                    ("Glasses", "None"),
                    ("Head", "Quiff"),
                    ("Hands", "None"),
                    ("Collectible", "None"),
                ])
                .into_traits();
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "WavyApe387");
                assert_eq!(
                    asset.image,
                    "ipfs://QmTXJg2xbRD7mzxzasRzbmVkFL5bXpZuav4tWbey8WkPwb"
                );
                assert_eq!(deserialized.data.total_supply, 4);
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_frog() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("frog.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("Body", "Blaze"),
                    ("Eyes", "Void"),
                    ("Head", "Dark Magic"),
                    ("Cloth", "Necromancer"),
                    ("Mouth", "ARRR"),
                    ("Rarity", "Common"),
                    ("Background", "Port Gore"),
                ])
                .into_traits();
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Frog #2618");
                assert_eq!(
                    asset.image,
                    "ipfs://QmXjszGijcAD4GniQSdrnySwkvF5GYUg2vQ34uCteUzjwg"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_nikeverse() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("nikeverse.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("tier", "Variant"),
                    ("background", "Pumba"),
                    ("body", "Lunar New Year"),
                    ("clothes", "Charles"),
                    ("eyes", "Patriot"),
                    ("nose", "Floral"),
                    ("head", "Psychedelic"),
                ])
                .into_traits();
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Nikeverse #1146");
                assert_eq!(
                    asset.image,
                    "ipfs://QmPYLFKuDt7fA2LJ1UHRodYz4f2dxZY1Sq6cVsDTS4J6gJ"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_wildtangs_s2() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("wildtangs_s2.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("fur", "White"),
                    ("body", "Default"),
                    ("eyes", "Angry"),
                    ("mouth", "Fruit Snack"),
                    ("eyewear", "Sunglasses"),
                    ("clothing", "Baseball Uniform"),
                    ("headwear", "Halo"),
                    ("background", "Avocado Green"),
                    ("accessories", "None"),
                    ("upgradedFrom", "WildTangz 623"),
                    (
                        "originalPolicy",
                        "33568ad11f93b3e79ae8dee5ad928ded72adcea719e92108caf1521b",
                    ),
                    ("artist", "@netanelchn"),
                ])
                .into_traits();
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Wild Tangz - Series 2 - #623");
                assert_eq!(
                    asset.image,
                    "ipfs://bafybeienp5gwmgxezesz2uu7pcawjk4nmnb5wftkn4rcst5debmpusxpdq"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_zenflow() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("zenflow.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    // Properties (explicit traits)
                    ("Agents", "Sweep"),
                    ("Coupling", "Tight"),
                    ("Crytaline", "Natural"),
                    ("Draw", "Normal"),
                    ("Filter", "Etheric"),
                    ("Form", "Neg Spiral"),
                    ("Palette", "P4"),
                    ("Rows", "Narrow"),
                    ("Waves", "W1"),
                    // Extra metadata fields merged as traits
                    ("artist", "Charles Machin - @CM_GenArt"),
                    ("authNFT", "asset13as3a4t954ru7vhzy6pldse3qu8e7la2xgsr7u"),
                    ("seed", "13719"),
                    ("vendor", "BlockGen.art"),
                    ("piece", "109"),
                    ("medium", "Fully On-Chain BlockGen.Art Canvas"),
                ])
                .into_traits();
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Zen Flow 109");
                assert_eq!(
                    asset.image,
                    "ipfs://Qmd7CZdPpYjZe8MuRRt4McvPUWWCK7bbhBcnYg8ZkZrqjf"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_nikeverse_reference() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("nikeverse_reference_nft.json"))
        {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("tier", "Champion"),
                    ("background", "Colosseum"),
                    ("body", "Fire"),
                    ("clothes", "Mummy"),
                    ("eyes", "Zombie"),
                    ("nose", "Mummy"),
                    ("head", "Fire"),
                ])
                .into_traits();
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Nikeverse #41144");
                assert_eq!(
                    asset.image,
                    "ipfs://QmSJuiN7Bkb44G9szbbzF7MN9SkhPCYA1MtDWedNSjGzsV"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_data_parse() {
        let request =
            serde_json::from_str::<AssetInfoResponse>(&test_case!("asset_info.json")).unwrap();

        let timestamp = request.data.first_mint_tx.timestamp.timestamp();
        assert_eq!(timestamp, 1735923628);
    }

    #[test]
    fn test_deserialize_owners_wavy() {
        match serde_json::from_str::<PolicyAccountsResponse>(&test_case!("owners_wavy.json")) {
            Ok(deserialized) => {
                assert_eq!(deserialized.data.len(), 100);
                assert_eq!(
                    deserialized.next_cursor,
                    Some("4dcWL_prWC61gmYBos1I9FDntmi72DUxVK1ZAFU".to_string())
                );
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_owners_wavy_57617679417065333837() {
        match serde_json::from_str::<AssetAccountsResponse>(&test_case!(
            "owners_wavy_57617679417065333837.json"
        )) {
            Ok(deserialized) => {
                assert_eq!(deserialized.data.len(), 4);
                assert_eq!(deserialized.next_cursor, None);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_policy_assets() {
        match serde_json::from_str::<PolicyAssetsResponse>(&test_case!(
            "policy_assets_response.json"
        )) {
            Ok(deserialized) => {
                assert_eq!(deserialized.data.len(), 100);
                assert_eq!(deserialized.get_importable_nfts().len(), 99);
                assert_eq!(deserialized.next_cursor, Some("UGlyYXRlMTIzMA".to_string()));
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_policy_assets_cip68() {
        match serde_json::from_str::<PolicyAssetsResponse>(&test_case!(
            "policy_assets_response_cip68.json"
        )) {
            Ok(deserialized) => {
                assert_eq!(deserialized.data.len(), 100);
                assert_eq!(deserialized.get_importable_nfts().len(), 0);
                assert_eq!(
                    deserialized.next_cursor,
                    Some("AAZDsE5pa2V2ZXJzZTAxMDI".to_string())
                );
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_policy_assets_pandas() {
        match serde_json::from_str::<PolicyAssetsResponse>(&test_case!(
            "policy_assets_response_pandas.json"
        )) {
            Ok(deserialized) => {
                assert_eq!(deserialized.data.len(), 100);
                assert_eq!(deserialized.get_importable_nfts().len(), 99);
                assert_eq!(
                    deserialized.next_cursor,
                    Some("UGFuZGFTb2NpZXR5MDA5OQ".to_string())
                );
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_spacebud() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("spacebud.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("type", "Cat"),
                    ("traits", "Chestplate, Belt, X-Ray"), // Array joined as single trait
                ])
                .into_traits();
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "SpaceBud #6023");
                assert_eq!(
                    asset.image,
                    "ipfs://bafkreig7mg6hfifafo6agma4s2nmkwbdvxdbqkjyyrzd5meqkoqsr2c5xq"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_derpbird() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("derpbird.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("Back", "Stoner"),
                    ("Background", "Red"),
                    ("Beak", "DJ"),
                    ("Body", "Astro"),
                    ("Color", "Orange"),
                    ("Ears", "Redneck"),
                    ("Eyes", "Redneck"),
                    ("Head", "Dave"),
                    ("Perfect Specimen", "No"),
                    ("Power Level", "Undetermined"),
                    ("Rarity", "Common"),
                    ("Tail", "Redneck"),
                    ("series", "2"), // Extra field included as trait
                ])
                .into_traits();
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Derp Bird #03821");
                assert_eq!(
                    asset.image,
                    "ipfs://Qma39EUpdgYqbSEKwwdYHDgAP221yQh19nBDzBAkvxeigE"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_claynation() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("claynation.json")) {
            Ok(deserialized) => {
                let test_traits = HashMap::from([
                    ("accessories", "ClayPods"),
                    ("background", "Lilac"),
                    ("body", "Tan Clay"),
                    ("brows", "Normal Eyebrows"),
                    ("clothes", "White Vest"),
                    ("eyes", "Love Glasses"),
                    ("hats and hair", "Clay Nation Cap"),
                    ("mouth", "Fly Tounge"),
                ])
                .into_traits();
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "Clay Nation #7103");
                assert_eq!(
                    asset.image,
                    "ipfs://QmbQZWhCkyazGHENa5Tzb3SnkjyPJJ8EFhXhds7jNMWVHN"
                );
                assert_eq!(asset.traits, test_traits);
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_unsig() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("unsig.json")) {
            Ok(deserialized) => {
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "unsig_12948");
                assert_eq!(
                    asset.image,
                    "ipfs://QmTeADGJVJSmzpa2AceMdZBDwzMf3w4BLAgqwx5ZZbw4cm"
                );

                // Check traits
                let traits = asset.traits.inner();
                assert_eq!(
                    traits.get("series"),
                    Some(&vec!["unsigned_algorithms".to_string()])
                );
                assert_eq!(traits.get("index"), Some(&vec!["12948".to_string()]));
                assert_eq!(traits.get("num_props"), Some(&vec!["5".to_string()]));
                assert_eq!(
                    traits.get("colors"),
                    Some(&vec![
                        "Blue".to_string(),
                        "Red".to_string(),
                        "Green".to_string(),
                        "Green".to_string(),
                        "Red".to_string()
                    ])
                );
                assert_eq!(
                    traits.get("distributions"),
                    Some(&vec![
                        "CDF".to_string(),
                        "CDF".to_string(),
                        "CDF".to_string(),
                        "CDF".to_string(),
                        "CDF".to_string()
                    ])
                );
                assert_eq!(
                    traits.get("multipliers"),
                    Some(&vec![
                        "0.5".to_string(),
                        "1".to_string(),
                        "2".to_string(),
                        "2".to_string(),
                        "4".to_string()
                    ])
                );
                assert_eq!(
                    traits.get("rotations"),
                    Some(&vec![
                        "90".to_string(),
                        "0".to_string(),
                        "0".to_string(),
                        "270".to_string(),
                        "270".to_string()
                    ])
                );
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }

    #[test]
    fn test_deserialize_blockowls() {
        match serde_json::from_str::<AssetInfoResponse>(&test_case!("blockowls.json")) {
            Ok(deserialized) => {
                let asset: Asset = deserialized.data.asset_standards.try_into().unwrap();
                assert_eq!(asset.name, "BlockOwls Razz");
                assert_eq!(
                    asset.image,
                    "ipfs://QmYocqqczwZx9tA2r4YmCrzsefg7GeeUwjRWzRoN8bPcVX"
                );

                // Check parsed colon-delimited traits
                let traits = asset.traits.inner();
                assert_eq!(traits.get("State"), Some(&vec!["Delusional".to_string()]));
                assert_eq!(
                    traits.get("Body Shape"),
                    Some(&vec!["ModifiedBlock".to_string()])
                );
                assert_eq!(
                    traits.get("Main Material"),
                    Some(&vec!["Silicone".to_string()])
                );
                assert_eq!(
                    traits.get("Display Box Base"),
                    Some(&vec!["BlackCardboard+Styrofoam".to_string()])
                );
                assert_eq!(
                    traits.get("Display Box Glass"),
                    Some(&vec!["PlainPlexi".to_string()])
                );
                assert_eq!(traits.get("Gender"), Some(&vec!["Female".to_string()]));
                assert_eq!(traits.get("number"), Some(&vec!["78".to_string()]));
                assert_eq!(traits.get("rarity"), Some(&vec!["Common".to_string()]));
            }
            Err(err) => {
                println!("encountered decoding error: {err:?}");
                panic!("failed decoding");
            }
        }
    }
}
