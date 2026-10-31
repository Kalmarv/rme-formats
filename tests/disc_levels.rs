//! The base levels of a SHRED disc read from MAIN.DB's `levels` table and the archives on the disc.
mod inputs;

use inputs::{Game, install};
use rme_formats::{database::Database, disc::Disc, manifest};

/// The table names thirteen level folders; ten have a base archive on the disc, the folder listing of which is the
/// reference.
#[test]
fn base_levels_are_the_tables_folders_on_the_disc() {
    let Some(root) = install(Game::Shred) else {
        return;
    };
    let disc = Disc::new(&root);
    let bytes = std::fs::read(disc.database()).unwrap();
    let rows = manifest::levels(&Database::open(&bytes).unwrap()).unwrap();
    let bases = disc.base_levels(&rows).unwrap();
    assert_eq!(
        bases.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "charlie", "delta", "echo", "foxtrot", "india", "juliet", "mike", "november", "oscar", "papa"
        ]
    );
    let mut without: Vec<String> = rows
        .iter()
        .filter_map(rme_formats::disc::level_folder)
        .filter(|f| !bases.contains_key(f))
        .collect();
    without.sort();
    without.dedup();
    assert_eq!(without, ["bravo", "golf", "hotel"]);
    assert_eq!(disc.layer_archives().unwrap().len(), 209);
}
