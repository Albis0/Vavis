//! Where generated files go.
//!
//! Files are written under the media directory with a sortable, unique name;
//! the database keeps only that name. Two rules hold the whole thing together:
//!
//! * **Nothing is overwritten.** A name collision produces a new name, never a
//!   silent replacement of a result the user may have liked.
//! * **Nothing escapes the media directory.** Paths come back out of the
//!   database, and a row edited by hand must not be able to delete
//!   `C:\Windows`.

use super::{Asset, Kind};
use std::io;
use std::path::{Path, PathBuf};

/// Builds the name for a new file: `2026-08-29_143012_image_7f3a.png`.
///
/// Date first so the directory sorts chronologically in any file manager,
/// which is how people actually look for "the one from last night".
pub fn file_name(kind: Kind, ext: &str, stamp: &str, unique: &str) -> String {
    format!("{stamp}_{}_{unique}.{ext}", kind.as_str())
}

/// Writes `asset` into `media_dir` and returns its name and size.
///
/// The name is relative: the data directory can move without invalidating
/// every row in the gallery.
pub fn save(media_dir: &Path, asset: &Asset, kind: Kind) -> io::Result<(String, u64)> {
    std::fs::create_dir_all(media_dir)?;

    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S").to_string();
    let mut name = file_name(kind, &asset.ext, &stamp, &unique_suffix());

    // Two images generated in the same second is normal — a batch of four
    // does it every time — so a taken name is retried, not overwritten.
    let mut attempt = 0;
    while media_dir.join(&name).exists() {
        attempt += 1;
        if attempt > 50 {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not find a free file name",
            ));
        }
        name = file_name(kind, &asset.ext, &stamp, &unique_suffix());
    }

    // Written to a temporary name first: a crash mid-write must not leave a
    // half-written file that the gallery then shows as a broken tile.
    let final_path = media_dir.join(&name);
    let temp_path = media_dir.join(format!("{name}.part"));
    std::fs::write(&temp_path, &asset.bytes)?;
    std::fs::rename(&temp_path, &final_path)?;

    Ok((name, asset.bytes.len() as u64))
}

/// Four hex characters from the system clock's sub-second part.
///
/// Not cryptographic, and it does not need to be: it only has to separate two
/// files written in the same second, and the caller retries on a collision.
fn unique_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    format!("{:04x}", nanos % 0x1_0000)
}

/// Resolves a stored name to a real path, refusing anything that leaves the
/// media directory.
///
/// The check is on the name rather than on the resolved path because the file
/// may already be gone — deleting a row whose file was removed by hand must
/// still work, and canonicalising a missing path fails.
pub fn resolve(media_dir: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }

    let candidate = Path::new(name);
    if candidate.is_absolute() {
        return None;
    }
    // Windows spellings of "somewhere else", refused on every platform:
    // `Path` only recognises its own OS's forms, so on anything but Windows
    // `C:\Windows` is one ordinary file name and `..\x` another. Names are
    // written by this app and never contain either separator or a colon, so
    // one that does is a hand-edited row, not a file of ours.
    if name.contains('\\') || name.contains(':') {
        return None;
    }

    for part in candidate.components() {
        use std::path::Component;
        match part {
            Component::Normal(_) => {}
            // `..`, a leading `/`, and `C:` are all ways out of the directory.
            _ => return None,
        }
    }

    Some(media_dir.join(candidate))
}

/// Deletes a stored file. A file that is already gone is a success: the caller
/// wants it absent, and it is.
pub fn remove(media_dir: &Path, name: &str) -> io::Result<()> {
    let Some(path) = resolve(media_dir, name) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path outside the media directory",
        ));
    };
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Reads a stored file back, for showing it in the interface.
pub fn read(media_dir: &Path, name: &str) -> io::Result<Vec<u8>> {
    let Some(path) = resolve(media_dir, name) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path outside the media directory",
        ));
    };
    std::fs::read(path)
}

/// Files in the media directory that no gallery row points at.
///
/// They accumulate from crashes and from rows deleted while the file was
/// locked. Left alone they are invisible disk usage, so the settings screen
/// can offer to clear them.
pub fn orphans(media_dir: &Path, known: &[String]) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(media_dir) else {
        return Vec::new();
    };

    entries
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| !known.contains(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(ext: &str) -> Asset {
        Asset {
            bytes: vec![1, 2, 3, 4],
            ext: ext.to_string(),
            seed: None,
            width: 8,
            height: 8,
        }
    }

    #[test]
    fn a_saved_file_is_named_by_date_and_kind() {
        let tmp = tempfile::tempdir().unwrap();
        let (name, bytes) = save(tmp.path(), &asset("png"), Kind::Image).unwrap();

        assert!(name.ends_with(".png"), "{name}");
        assert!(name.contains("_image_"), "{name}");
        assert_eq!(bytes, 4);
        assert!(tmp.path().join(&name).exists());
    }

    #[test]
    fn a_batch_written_in_one_second_does_not_overwrite_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let mut names = Vec::new();
        for _ in 0..8 {
            names.push(save(tmp.path(), &asset("png"), Kind::Image).unwrap().0);
        }

        names.sort();
        names.dedup();
        assert_eq!(names.len(), 8, "a name was reused and a result was lost");
    }

    #[test]
    fn saving_leaves_no_partial_file_behind() {
        let tmp = tempfile::tempdir().unwrap();
        save(tmp.path(), &asset("png"), Kind::Image).unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn video_is_named_as_video() {
        let tmp = tempfile::tempdir().unwrap();
        let (name, _) = save(tmp.path(), &asset("mp4"), Kind::Video).unwrap();
        assert!(name.contains("_video_"), "{name}");
    }

    #[test]
    fn saving_creates_the_directory_if_it_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("not-yet");
        save(&missing, &asset("png"), Kind::Image).unwrap();
        assert!(missing.exists());
    }

    #[test]
    fn a_stored_name_resolves_under_the_media_directory() {
        let media = Path::new("/data/media");
        assert_eq!(
            resolve(media, "a.png"),
            Some(PathBuf::from("/data/media/a.png"))
        );
    }

    #[test]
    fn a_path_that_climbs_out_is_refused() {
        let media = Path::new("/data/media");
        // A row edited by hand must not be able to reach outside.
        assert_eq!(resolve(media, "../../etc/passwd"), None);
        assert_eq!(resolve(media, "sub/../../out.png"), None);
        assert_eq!(resolve(media, ""), None);
    }

    #[test]
    fn an_absolute_path_is_refused() {
        let media = Path::new("/data/media");
        assert_eq!(resolve(media, "/etc/passwd"), None);
        assert_eq!(resolve(media, "C:\\Windows\\system32"), None);
    }

    #[test]
    fn removing_a_file_that_is_already_gone_is_fine() {
        let tmp = tempfile::tempdir().unwrap();
        // The user wants it absent, and it is. Reporting an error here would
        // leave undeletable rows in the gallery.
        remove(tmp.path(), "never-existed.png").unwrap();
    }

    #[test]
    fn removing_outside_the_media_directory_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().parent().unwrap().join("victim.txt");
        std::fs::write(&outside, "important").unwrap();

        let media = tmp.path().join("media");
        std::fs::create_dir_all(&media).unwrap();

        assert!(remove(&media, "../victim.txt").is_err());
        assert!(outside.exists(), "a traversal deleted a file outside");

        std::fs::remove_file(&outside).ok();
    }

    #[test]
    fn a_saved_file_reads_back_byte_for_byte() {
        let tmp = tempfile::tempdir().unwrap();
        let (name, _) = save(tmp.path(), &asset("png"), Kind::Image).unwrap();
        assert_eq!(read(tmp.path(), &name).unwrap(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn orphans_are_the_files_no_row_points_at() {
        let tmp = tempfile::tempdir().unwrap();
        let (kept, _) = save(tmp.path(), &asset("png"), Kind::Image).unwrap();
        std::fs::write(tmp.path().join("stray.png"), b"x").unwrap();

        let found = orphans(tmp.path(), std::slice::from_ref(&kept));
        assert_eq!(found, vec!["stray.png"]);
    }

    #[test]
    fn orphans_of_a_missing_directory_is_empty_not_an_error() {
        assert!(orphans(Path::new("/definitely/not/here"), &[]).is_empty());
    }
}
