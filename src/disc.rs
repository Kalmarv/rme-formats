//! Where the game's files are on the PS3 disc: the `PS3_GAME/USRDIR` folder and the archives and database in it.
//!
//! `USRDIR/LEVELS/<LEVEL>.PAK` is a level's base archive and `USRDIR/LEVELS/LAYERS/<DSZZ>.PAK` its layer archives (see
//! [`crate::manifest`] for what the names mean); `USRDIR/DATABASE/MAIN.DB` is the game's database and
//! `USRDIR/DATABASE/MATERIALS.PAK` its material banks. The disc's file names are upper case as the game names them;
//! an extracted copy may hold them in any case, so they are looked up without regard to case, and two names that
//! differ only in case are refused.
//!
//! A level is one the database's `levels` table names by its `PATH` folder (`levels\Charlie\`) whose base archive is
//! on the disc ([`Disc::base_levels`]). The table names more level folders than the disc holds bases for: Bravo, Golf
//! and Hotel are listed with no base archive on the disc.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "reader I/O: list the caller's disc folder"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::manifest::{LevelRow, parse_layer_name};
use crate::{Result, invalid};

/// The disc's game folder, below the folder that holds `PS3_GAME`.
pub const USRDIR: &str = "PS3_GAME/USRDIR";

/// The archives at the top of the folder that a level's materials draw on besides its own archives (fragment programs
/// and textures the banks name): `RME.PAK` and `VFX.PAK`, lower case as listed here. Observed, not read: no table of
/// the data is known to say which archives a level's materials need; these two are the ones the material reads of the
/// checked levels were made with.
pub const SHARED_ARCHIVES: [&str; 2] = ["rme.pak", "vfx.pak"];

/// A disc folder: the folder that holds `PS3_GAME`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disc {
    usrdir: PathBuf,
}

impl Disc {
    /// The disc whose `PS3_GAME` is in `folder`. Nothing is read until a listing is asked for.
    pub fn new(folder: impl AsRef<Path>) -> Self {
        Disc {
            usrdir: folder.as_ref().join(USRDIR),
        }
    }

    /// The `PS3_GAME/USRDIR` folder.
    pub fn usrdir(&self) -> &Path {
        &self.usrdir
    }

    /// The game's database, `DATABASE/MAIN.DB` ([`crate::database`]).
    pub fn database(&self) -> PathBuf {
        self.usrdir.join("database/main.db")
    }

    /// The material banks, `DATABASE/MATERIALS.PAK` ([`crate::material`]).
    pub fn material_banks(&self) -> PathBuf {
        self.usrdir.join("database/materials.pak")
    }

    /// The [`SHARED_ARCHIVES`], each with its path.
    pub fn shared_archives(&self) -> Vec<(&'static str, PathBuf)> {
        SHARED_ARCHIVES
            .iter()
            .map(|name| (*name, self.usrdir.join(name)))
            .collect()
    }

    /// The base archives of the levels of `levels` (the database's `levels` rows, [`crate::manifest::levels`]) that
    /// are on the disc: each level folder's name, lower case, with the path of its `LEVELS/<LEVEL>.PAK`. A level folder
    /// with no base archive is left out; a row with no `PATH` folder names none.
    pub fn base_levels(&self, levels: &[LevelRow]) -> Result<BTreeMap<String, PathBuf>> {
        let folders: Vec<String> = levels.iter().filter_map(level_folder).collect();
        let dir = self.usrdir.join("levels");
        let mut out = BTreeMap::new();
        for (name, path) in files(&dir)? {
            let Some(stem) = name.strip_suffix(".pak") else {
                continue;
            };
            if folders.iter().any(|f| f == stem) && out.insert(stem.to_owned(), path).is_some() {
                return Err(invalid(
                    0,
                    format!("{}: two base archives named {stem}", dir.display()),
                ));
            }
        }
        Ok(out)
    }

    /// The layer archives, `LEVELS/LAYERS/<DSZZ>.PAK` (names [`parse_layer_name`] reads), by lower-case file name with
    /// their paths; none when the disc has no `LAYERS` folder.
    pub fn layer_archives(&self) -> Result<BTreeMap<String, PathBuf>> {
        let dir = self.usrdir.join("levels/layers");
        if !dir.is_dir() {
            return Ok(BTreeMap::new());
        }
        let mut out = BTreeMap::new();
        for (name, path) in files(&dir)? {
            if parse_layer_name(&name).is_some() && out.insert(name.clone(), path).is_some() {
                return Err(invalid(
                    0,
                    format!("{}: two layer archives named {name}", dir.display()),
                ));
            }
        }
        Ok(out)
    }
}

/// The level folder a `levels` row's `PATH` names (`levels\Charlie\`: `charlie`), lower case.
pub fn level_folder(row: &LevelRow) -> Option<String> {
    let folder = row.path.strip_prefix("levels\\")?.trim_end_matches('\\');
    (!folder.is_empty() && !folder.contains('\\')).then(|| folder.to_ascii_lowercase())
}

/// The files of `dir`, each by its lower-case name, in listing order.
fn files(dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_ascii_lowercase) else {
            continue;
        };
        out.push((name, entry.path()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, path: &str) -> LevelRow {
        LevelRow {
            id: 1,
            name: name.into(),
            display_name: name.into(),
            path: path.into(),
            board_type: String::new(),
            sessions: Vec::new(),
            script: String::new(),
        }
    }

    #[test]
    fn a_rows_path_names_its_level_folder() {
        assert_eq!(
            level_folder(&row("Charlie", "levels\\Charlie\\")).as_deref(),
            Some("charlie")
        );
        assert_eq!(
            level_folder(&row("Mike", "levels\\Mike")).as_deref(),
            Some("mike")
        );
        assert_eq!(level_folder(&row("PlayerIsAwesome", "")), None);
        assert_eq!(level_folder(&row("Odd", "levels\\a\\b\\")), None);
        assert_eq!(level_folder(&row("Odd", "data\\Charlie\\")), None);
    }

    #[test]
    fn base_levels_are_the_tables_folders_with_an_archive() {
        let dir = std::env::temp_dir().join(format!("rme-formats-disc-{}", std::process::id()));
        let levels = dir.join(USRDIR).join("levels");
        std::fs::create_dir_all(levels.join("layers")).unwrap();
        for name in ["CHARLIE.PAK", "mike.pak", "unknown.pak", "oscar.txt"] {
            std::fs::write(levels.join(name), []).unwrap();
        }
        for name in ["8100000C.PAK", "notalayer.pak"] {
            std::fs::write(levels.join("layers").join(name), []).unwrap();
        }
        let disc = Disc::new(&dir);
        let rows = [
            row("Charlie", "levels\\Charlie\\"),
            row("Mike", "levels\\Mike\\"),
            row("Oscar", "levels\\Oscar\\"),
            row("Bravo", "levels\\Bravo\\"),
        ];
        let bases = disc.base_levels(&rows).unwrap();
        assert_eq!(
            bases.keys().map(String::as_str).collect::<Vec<_>>(),
            ["charlie", "mike"]
        );
        let layers = disc.layer_archives().unwrap();
        assert_eq!(
            layers.keys().map(String::as_str).collect::<Vec<_>>(),
            ["8100000c.pak"]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
