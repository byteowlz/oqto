//! Raster project logos for the personal-host Projects grid.
//!
//! Only Work directories present in the authorized Session catalog are served,
//! from their `logo/` folder, with the Server's naming conventions: square marks
//! (no `banner`/`name`/`slogan`/`wordmark` in the name) before others, PNG/JPEG
//! only (phones have no general SVG decoder), at most three candidates per
//! directory and `MAX_PROJECT_ICON_BYTES` each. Symlinks are never followed;
//! an unreadable or oversized logo is simply absent.
use base64::Engine;
use oqto_history::oqto_log::ops::OqtoLogSessionRow;
use oqto_personal_wire::actions::{MAX_PROJECT_ICON_BYTES, MAX_PROJECT_ICON_DIRECTORIES};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

const CANDIDATES: usize = 3;
const NOT_SQUARE: [&str; 4] = ["banner", "name", "slogan", "wordmark"];

pub(crate) fn project_icons(directories: &[String], rows: &[OqtoLogSessionRow]) -> Option<Value> {
    if directories.len() > MAX_PROJECT_ICON_DIRECTORIES {
        return None;
    }
    let known: HashSet<&str> = rows
        .iter()
        .filter_map(|row| row.workspace_id.as_deref())
        .collect();
    let mut seen = HashSet::new();
    let icons: Vec<Value> = directories
        .iter()
        .filter(|directory| known.contains(directory.as_str()) && seen.insert(directory.as_str()))
        .filter_map(|directory| {
            let candidates = candidates(Path::new(directory));
            (!candidates.is_empty())
                .then(|| json!({"directory": directory, "candidates": candidates}))
        })
        .collect();
    Some(json!({"icons": icons}))
}

fn candidates(directory: &Path) -> Vec<Value> {
    let logo = directory.join("logo");
    // The logo folder itself must be a real directory, not a link elsewhere.
    if !std::fs::symlink_metadata(&logo).is_ok_and(|meta| meta.is_dir()) {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(&logo) else {
        return Vec::new();
    };
    let mut files: Vec<(bool, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            mime(&name)?;
            let stem = name
                .rsplit_once('.')
                .map_or(name.as_str(), |(stem, _)| stem)
                .to_ascii_lowercase();
            let square = !stem
                .split(['_', '-'])
                .any(|word| NOT_SQUARE.contains(&word));
            Some((square, name))
        })
        .collect();
    files.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.len().cmp(&b.1.len()))
            .then(a.1.cmp(&b.1))
    });
    files
        .into_iter()
        .filter_map(|(_, name)| {
            let bytes = read_bounded(&logo.join(&name))?;
            Some(json!({
                "name": name,
                "mime": mime(&name)?,
                "data_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
            }))
        })
        .take(CANDIDATES)
        .collect()
}

fn mime(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".png") {
        Some("image/png")
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("image/jpeg")
    } else {
        None
    }
}

fn read_bounded(path: &Path) -> Option<Vec<u8>> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_PROJECT_ICON_BYTES as u64 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_PROJECT_ICON_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= MAX_PROJECT_ICON_BYTES).then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(workspace: &str) -> OqtoLogSessionRow {
        OqtoLogSessionRow {
            session_id: "s".into(),
            platform_id: "oqto-s".into(),
            external_id: Some("n".into()),
            user_id: "owner".into(),
            workspace_id: Some(workspace.into()),
            parent_session_id: None,
            forked_from_entry_id: None,
            forked_at: None,
            created_at: String::new(),
            updated_at: String::new(),
            messages: 0,
            title: None,
            readable_id: None,
        }
    }

    #[test]
    fn serves_only_catalog_directories_square_first_raster_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("app");
        let logo = app.join("logo");
        std::fs::create_dir_all(&logo).unwrap();
        std::fs::write(logo.join("app_logo_banner_white.png"), b"banner").unwrap();
        std::fs::write(logo.join("app_logo_white.png"), b"square").unwrap();
        std::fs::write(logo.join("app_logo.svg"), b"<svg/>").unwrap();
        std::fs::write(logo.join("huge.png"), vec![0u8; MAX_PROJECT_ICON_BYTES + 1]).unwrap();
        std::fs::write(logo.join("mark.jpg"), b"jpeg").unwrap();
        std::os::unix::fs::symlink(logo.join("mark.jpg"), logo.join("a.png")).unwrap();
        let other = dir.path().join("other");
        std::fs::create_dir_all(other.join("logo")).unwrap();
        std::fs::write(other.join("logo/x.png"), b"secret").unwrap();
        let app_path = app.to_str().unwrap().to_owned();
        let other_path = other.to_str().unwrap().to_owned();
        let value = project_icons(
            &[
                app_path.clone(),
                other_path,
                app_path.clone(),
                "/nowhere".into(),
            ],
            &[row(&app_path)],
        )
        .unwrap();
        let icons = value["icons"].as_array().unwrap();
        assert_eq!(
            icons.len(),
            1,
            "uncatalogued and duplicate directories are not served"
        );
        assert_eq!(icons[0]["directory"], app_path);
        let names: Vec<_> = icons[0]["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        // Square marks first, then the banner; never SVG, oversized or symlinked files.
        assert_eq!(
            names,
            [
                "mark.jpg",
                "app_logo_white.png",
                "app_logo_banner_white.png"
            ]
        );
        let first = &icons[0]["candidates"][0];
        assert_eq!(first["mime"], "image/jpeg");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(first["data_base64"].as_str().unwrap())
                .unwrap(),
            b"jpeg"
        );
    }

    #[test]
    fn linked_logo_folder_and_oversized_requests_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("logo.png"), b"x").unwrap();
        let app = dir.path().join("app");
        std::fs::create_dir_all(&app).unwrap();
        std::os::unix::fs::symlink(&elsewhere, app.join("logo")).unwrap();
        let app_path = app.to_str().unwrap().to_owned();
        let value = project_icons(std::slice::from_ref(&app_path), &[row(&app_path)]).unwrap();
        assert_eq!(value["icons"], json!([]));
        let many = vec![app_path; MAX_PROJECT_ICON_DIRECTORIES + 1];
        assert!(project_icons(&many, &[]).is_none());
    }
}
