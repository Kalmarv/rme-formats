//! Lists the level meshes of a PAK archive from a PS3 disc of Tony Hawk's SHRED.
//!
//! ```text
//! cargo run --example list_quartets -- path/to/PS3_GAME/USRDIR/LEVELS/CHARLIE.PAK
//! ```
use rme_formats::{pak::Pak, scene};

fn main() -> Result<(), rme_formats::Error> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: list_quartets <archive.pak>");
    let pak = Pak::open(path)?;
    for (stem, entries) in scene::quartets(&pak)? {
        let scene = scene::decode_quartet(&pak, entries)?;
        println!(
            "{}: {} models, {} placements, {} unread spans",
            String::from_utf8_lossy(&stem),
            scene.models.len(),
            scene.placements.len(),
            scene.unread.len()
        );
    }
    Ok(())
}
