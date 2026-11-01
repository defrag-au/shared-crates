//! Stamp the app's plugin JS into `web/`, so the page loads the plugins the
//! wasm was linked against: `wallet.js`, `evm.js` and `midnight.js`
//! (wallet-miniquad), `platform.js` (miniquad-platform), `app.js`
//! (macroquad-app's HTTP) and `discord-auth.js` (the session a Discord login
//! hands back). `mq-bundle --web ui/macroquad-wallet-link/web` stages every
//! `*.js` there. Generated, so `web/` is gitignored.

use std::fs;
use std::path::Path;

fn main() {
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
    fs::create_dir_all(&web).expect("create web/");
    fs::write(web.join("wallet.js"), wallet_miniquad::PLUGIN_JS).expect("write wallet.js");
    fs::write(web.join("evm.js"), wallet_miniquad::EVM_PLUGIN_JS).expect("write evm.js");
    fs::write(web.join("midnight.js"), wallet_miniquad::MIDNIGHT_PLUGIN_JS)
        .expect("write midnight.js");
    fs::write(web.join("platform.js"), miniquad_platform::PLUGIN_JS).expect("write platform.js");
    fs::write(web.join("app.js"), macroquad_app::PLUGIN_JS).expect("write app.js");
    fs::write(
        web.join("discord-auth.js"),
        discord_auth::miniquad::PLUGIN_JS,
    )
    .expect("write discord-auth.js");
    println!("cargo:rerun-if-changed=build.rs");
    // The JS these consts carry lives in the crates' own directories, and cargo
    // re-runs a build script only for the files it is told to watch. Without
    // these, editing a plugin's JS leaves `web/` holding the previous copy, and
    // the bundle ships a page whose bridge is a version behind its wasm — the
    // exact silent mismatch `mq-bundle`'s import check cannot see, because the
    // imports are still provided, just by older code.
    for js in [
        "../wallet-miniquad/js/wallet.js",
        "../wallet-miniquad/js/evm.js",
        "../wallet-miniquad/js/midnight.js",
        "../miniquad-platform/js/platform.js",
        "../macroquad-app/js/app.js",
        "../../discord-auth/js/discord-auth.js",
    ] {
        println!("cargo:rerun-if-changed={js}");
    }
}
