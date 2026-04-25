//! `garagetytus about` — Phase B.5 AGPL surface.
//!
//! Prints version + bundled Garage version + upstream source URL +
//! NOTICES location. Must run on every supported OS even before
//! `garagetytus install` has been invoked, so all values are baked
//! at compile time.

use anyhow::Result;

const GARAGETYTUS_VERSION: &str = env!("CARGO_PKG_VERSION");
const GARAGE_VERSION: &str = "v2.3.0";
const GARAGE_SOURCE_URL: &str = "https://git.deuxfleurs.fr/Deuxfleurs/garage";
const GARAGE_LICENSE: &str = "AGPL-3.0-or-later";
const GARAGE_TARBALL_SHA: &str =
    "b83a981677676b35400bbbaf20974c396f32da31c7c7630ce55fc3e62c0e2e01";

pub fn run() -> Result<i32> {
    println!("garagetytus {}", GARAGETYTUS_VERSION);
    println!();
    println!("Bundled Garage:");
    println!("  version       {}", GARAGE_VERSION);
    println!("  source        {}/src/tag/{}", GARAGE_SOURCE_URL, GARAGE_VERSION);
    println!("  license       {}", GARAGE_LICENSE);
    println!("  tarball SHA   {}", GARAGE_TARBALL_SHA);
    println!();
    println!("On macOS, Garage is acquired via Homebrew");
    println!("(brew install traylinx/tap/garagetytus depends on `garage`).");
    println!("On Linux, the upstream `*-musl/garage` binary is downloaded");
    println!("at `garagetytus install` time and pinned to the SHA above.");
    println!();
    println!("THIRD_PARTY_NOTICES location: see the install tarball or");
    println!("https://github.com/traylinx/garagetytus/blob/main/THIRD_PARTY_NOTICES");
    Ok(0)
}
