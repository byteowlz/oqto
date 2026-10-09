//! Canonical portable-path rules shared by manifest, capability, and
//! operations validation.
//!
//! Every path that reaches a Definition, a grant, or the runner must be a
//! portable relative path built only from normal components. Callers add their
//! own bounds and reserved-name policy on top of these base rules.

use std::path::{Component, Path, PathBuf};

use crate::{AppPackageError, AppPackageErrorCode};

/// Bounds applied to one validated relative path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PathLimits {
    pub max_bytes: usize,
    pub max_components: usize,
}

/// Validate a portable relative path with no bound on length or depth.
pub(crate) fn portable_relative_path(value: &str) -> Result<PathBuf, AppPackageError> {
    if value.is_empty() || value.contains('\\') {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidRelativePath,
            "path must be a non-empty portable relative path",
        ));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidRelativePath,
            "path contains an absolute, dot, parent, or platform-prefix component",
        ));
    }
    Ok(path.to_path_buf())
}

/// Validate a portable relative path and enforce byte and component bounds.
pub(crate) fn bounded_relative_path(
    value: &str,
    limits: PathLimits,
) -> Result<PathBuf, AppPackageError> {
    if value.len() > limits.max_bytes {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidRelativePath,
            format!(
                "path is {} bytes; limit is {}",
                value.len(),
                limits.max_bytes
            ),
        ));
    }
    let path = portable_relative_path(value)?;
    let component_count = path.components().count();
    if component_count > limits.max_components {
        return Err(AppPackageError::new(
            AppPackageErrorCode::InvalidRelativePath,
            format!(
                "path has {component_count} components; limit is {}",
                limits.max_components
            ),
        ));
    }
    Ok(path)
}

/// True when `path` lives strictly beneath the single-component `root`.
pub(crate) fn is_beneath(path: &Path, root: &str) -> bool {
    let mut components = path.components();
    if components.next() != Some(Component::Normal(root.as_ref())) {
        return false;
    }
    components.next().is_some()
}

/// Portable string form used in diagnostics and stable file indexes.
#[must_use]
pub(crate) fn portable_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_escapes_and_platform_paths() {
        for value in ["", "../secret", "/etc/passwd", "a\\b", "./a", "a/../b"] {
            assert_eq!(
                portable_relative_path(value).expect_err("must reject").code,
                AppPackageErrorCode::InvalidRelativePath,
                "value={value}"
            );
        }
    }

    #[test]
    fn enforces_byte_and_component_bounds() {
        let limits = PathLimits {
            max_bytes: 8,
            max_components: 2,
        };
        assert!(bounded_relative_path("a/b", limits).is_ok());
        assert!(bounded_relative_path("aaaaaaaaaaaa", limits).is_err());
        assert!(bounded_relative_path("a/b/c", limits).is_err());
    }

    #[test]
    fn beneath_requires_a_child_of_the_root() {
        assert!(is_beneath(Path::new("operations/table.toml"), "operations"));
        assert!(is_beneath(Path::new("operations/bin/tool"), "operations"));
        assert!(!is_beneath(Path::new("operations"), "operations"));
        assert!(!is_beneath(Path::new("bundle/index.html"), "operations"));
    }
}
