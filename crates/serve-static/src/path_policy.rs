use std::ffi::OsStr;
#[cfg(not(windows))]
use std::ffi::OsString;
use std::fs::Metadata;
use std::path::{Component, Path, PathBuf};

/// Controls whether static directory requests may follow symbolic links or junctions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum SymlinkPolicy {
    /// Reject symbolic links and junctions below a configured root.
    #[default]
    Deny,
    /// Follow symbolic links and junctions only when their targets remain within the root.
    WithinRoot,
}

#[derive(Debug)]
pub(crate) enum Resolution {
    Found { path: PathBuf, metadata: Metadata },
    Missing,
    Rejected,
}

pub(crate) fn valid_fs_component(part: &str) -> bool {
    #[cfg(windows)]
    {
        if part.contains(':') {
            return false;
        }
        let base = part.split('.').next().unwrap_or(part).trim_end_matches(' ');
        let numbered = base.get(..3).is_some_and(|prefix| {
            prefix.eq_ignore_ascii_case("COM") || prefix.eq_ignore_ascii_case("LPT")
        }) && matches!(
            base.get(3..),
            Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
        );
        !numbered
            && !["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
                .iter()
                .any(|name| base.eq_ignore_ascii_case(name))
    }
    #[cfg(not(windows))]
    {
        let _ = part;
        true
    }
}

/// Counts both visible listing entries and internal link-target name lookups.
pub(crate) struct EntryBudget {
    remaining: usize,
    exceeded: bool,
}

impl EntryBudget {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            remaining: limit,
            exceeded: false,
        }
    }

    pub(crate) fn visit(&mut self) -> bool {
        if self.remaining == 0 {
            self.exceeded = true;
            false
        } else {
            self.remaining -= 1;
            true
        }
    }

    pub(crate) fn exceeded(&self) -> bool {
        self.exceeded
    }
}

/// Resolves a relative path beneath an already canonical, absolute configured root.
pub(crate) async fn resolve(
    root: &Path,
    relative: &Path,
    strict: bool,
    symlinks: SymlinkPolicy,
) -> Resolution {
    // Validate the entire path before any lookup, including components after an
    // otherwise missing entry. Joining rooted paths would discard the root.
    let mut components = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(name) => {
                #[cfg(windows)]
                {
                    let bytes = name.as_encoded_bytes();
                    // A drive designator in a later component must not reset
                    // PathBuf::join, and ambiguous Win32 suffixes are terminal.
                    if !valid_fs_component(&name.to_string_lossy())
                        || bytes.contains(&0)
                        || strict && bytes.last().is_some_and(|byte| matches!(byte, b'.' | b' '))
                    {
                        return Resolution::Rejected;
                    }
                }
                components.push(name);
            }
            Component::CurDir => {}
            _ => return Resolution::Rejected,
        }
    }

    let mut current = root.to_path_buf();
    for name in components {
        let candidate = current.join(name);
        let Ok(metadata) = tokio::fs::symlink_metadata(&candidate).await else {
            return Resolution::Missing;
        };
        // POSIX realpath can retain the caller's spelling on case-insensitive
        // mounts (including NTFS through WSL). Verify the directory entry itself.
        // In relaxed mode recover its real spelling for resource authorization.
        #[cfg(not(windows))]
        let candidate = match actual_entry_name(&current, name, &metadata, strict, None).await {
            Ok(Some(actual)) => current.join(actual),
            Ok(None) => return Resolution::Rejected,
            Err(_) => return Resolution::Missing,
        };
        // On Windows, std also classifies name-surrogate reparse points such
        // as junctions as symlinks, without rejecting ordinary cloud files.
        let is_symlink = metadata.file_type().is_symlink();
        if is_symlink && symlinks == SymlinkPolicy::Deny {
            return Resolution::Rejected;
        }

        #[cfg(windows)]
        if strict && is_symlink {
            // A link's canonical filename belongs to its target. Check the
            // directory entry instead so strict mode validates the link name.
            match has_exact_entry(&current, name, None).await {
                Ok(true) => {}
                Ok(false) => return Resolution::Rejected,
                Err(_) => return Resolution::Missing,
            }
        }

        let Ok(canonical) = tokio::fs::canonicalize(&candidate).await else {
            return Resolution::Missing;
        };
        if !canonical.starts_with(root) {
            return Resolution::Rejected;
        }
        #[cfg(windows)]
        if strict && !is_symlink && canonical.file_name() != Some(name) {
            return Resolution::Rejected;
        }
        #[cfg(not(windows))]
        let canonical = if is_symlink {
            match canonical_target_spelling(root, &canonical, None).await {
                Ok(Some(path)) => path,
                Ok(None) => return Resolution::Rejected,
                Err(_) => return Resolution::Missing,
            }
        } else {
            canonical
        };
        current = canonical;
    }

    // This also handles an empty relative path: the configured root is the
    // mount itself and may have been explicitly configured through a link.
    match tokio::fs::metadata(&current).await {
        Ok(metadata) => Resolution::Found {
            path: current,
            metadata,
        },
        Err(_) => Resolution::Missing,
    }
}

/// Resolve an entry obtained by enumerating an already resolved directory.
/// Its name is exact, so ordinary entries need no second parent-directory scan.
#[cfg_attr(windows, allow(clippy::needless_pass_by_ref_mut))]
pub(crate) async fn resolve_entry(
    root: &Path,
    entry: &tokio::fs::DirEntry,
    strict: bool,
    symlinks: SymlinkPolicy,
    budget: &mut EntryBudget,
) -> Resolution {
    let candidate = entry.path();
    if !candidate.starts_with(root) {
        return Resolution::Rejected;
    }
    #[cfg(windows)]
    if strict
        && entry
            .file_name()
            .as_encoded_bytes()
            .last()
            .is_some_and(|byte| matches!(byte, b'.' | b' '))
    {
        return Resolution::Rejected;
    }
    #[cfg(not(windows))]
    let _ = strict;
    #[cfg(windows)]
    let _ = budget;
    let Ok(metadata) = tokio::fs::symlink_metadata(&candidate).await else {
        return Resolution::Missing;
    };
    if !metadata.file_type().is_symlink() {
        return Resolution::Found {
            path: candidate,
            metadata,
        };
    }
    if symlinks == SymlinkPolicy::Deny {
        return Resolution::Rejected;
    }
    let Ok(canonical) = tokio::fs::canonicalize(candidate).await else {
        return Resolution::Missing;
    };
    if !canonical.starts_with(root) {
        return Resolution::Rejected;
    }
    #[cfg(not(windows))]
    let canonical = match canonical_target_spelling(root, &canonical, Some(budget)).await {
        Ok(Some(path)) => path,
        Ok(None) => return Resolution::Rejected,
        Err(_) => return Resolution::Missing,
    };
    match tokio::fs::metadata(&canonical).await {
        Ok(metadata) => Resolution::Found {
            path: canonical,
            metadata,
        },
        Err(_) => Resolution::Missing,
    }
}

async fn has_exact_entry(
    parent: &Path,
    name: &OsStr,
    mut budget: Option<&mut EntryBudget>,
) -> std::io::Result<bool> {
    let mut entries = tokio::fs::read_dir(parent).await?;
    while let Some(entry) = entries.next_entry().await? {
        count_entry(budget.as_deref_mut())?;
        if entry.file_name() == name {
            return Ok(true);
        }
    }
    Ok(false)
}

fn count_entry(budget: Option<&mut EntryBudget>) -> std::io::Result<()> {
    if budget.is_some_and(|budget| !budget.visit()) {
        Err(std::io::Error::other(
            "directory listing entry limit exceeded",
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
async fn actual_entry_name(
    parent: &Path,
    name: &OsStr,
    metadata: &Metadata,
    strict: bool,
    mut budget: Option<&mut EntryBudget>,
) -> std::io::Result<Option<OsString>> {
    if has_exact_entry(parent, name, budget.as_deref_mut()).await? {
        return Ok(Some(name.to_os_string()));
    }
    if strict {
        return Ok(None);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mut entries = tokio::fs::read_dir(parent).await?;
        let mut actual = None;
        while let Some(entry) = entries.next_entry().await? {
            count_entry(budget.as_deref_mut())?;
            let found = tokio::fs::symlink_metadata(entry.path()).await?;
            if found.dev() == metadata.dev() && found.ino() == metadata.ino() {
                // Multiple hard-link names do not establish a unique spelling
                // for an implicit alias. Require the caller to use an exact name.
                if actual.is_some() {
                    return Ok(None);
                }
                actual = Some(entry.file_name());
            }
        }
        Ok(actual)
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        Ok(None)
    }
}

#[cfg(not(windows))]
async fn canonical_target_spelling(
    root: &Path,
    path: &Path,
    mut budget: Option<&mut EntryBudget>,
) -> std::io::Result<Option<PathBuf>> {
    let Ok(relative) = path.strip_prefix(root) else {
        return Ok(None);
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Ok(None);
        };
        let candidate = current.join(name);
        let metadata = tokio::fs::symlink_metadata(&candidate).await?;
        if metadata.file_type().is_symlink() {
            return Ok(None);
        }
        let Some(actual) =
            actual_entry_name(&current, name, &metadata, false, budget.as_deref_mut()).await?
        else {
            return Ok(None);
        };
        current.push(actual);
    }
    Ok(Some(current))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn filesystem_components_are_validated_at_the_resource_boundary() {
        for part in [
            "C:",
            "file:stream",
            "CON",
            "con.txt",
            "CON .txt",
            "PRN",
            "AUX",
            "NUL",
            "COM1",
            "LPT9",
            "COM¹",
            "LPT²",
            "CONIN$",
            "CONOUT$",
        ] {
            assert_eq!(valid_fs_component(part), !cfg!(windows), "{part}");
        }
        for part in ["CONSOLE", "COM10", "LPT0", "file.txt", "用户"] {
            assert!(valid_fs_component(part), "{part}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_device_and_stream_names_are_rejected_at_the_resource_boundary() {
        for part in [
            "C:",
            "file:stream",
            "CON",
            "con.txt",
            "CON .txt",
            "PRN",
            "AUX",
            "NUL",
            "COM1",
            "COM9",
            "LPT1",
            "LPT9",
            "COM¹",
            "LPT²",
            "CONIN$",
            "CONOUT$",
        ] {
            assert!(!valid_fs_component(part), "{part}");
        }
        for part in ["CONSOLE", "COM10", "LPT10", "ordinary.txt", "文件"] {
            assert!(valid_fs_component(part), "{part}");
        }
    }

    #[tokio::test]
    async fn rejects_non_relative_components_before_looking_up_entries() {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();

        for relative in [Path::new("../outside"), Path::new("missing/../file"), &root] {
            assert!(matches!(
                resolve(&root, relative, false, SymlinkPolicy::WithinRoot).await,
                Resolution::Rejected
            ));
        }

        #[cfg(windows)]
        for relative in [r"C:relative", r"\rooted", r"\\server\share\file"] {
            assert!(matches!(
                resolve(&root, Path::new(relative), false, SymlinkPolicy::WithinRoot).await,
                Resolution::Rejected
            ));
        }
    }

    #[tokio::test]
    async fn resolves_the_root_and_returns_final_target_metadata() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("subdirectory")).unwrap();
        fs::write(directory.path().join("subdirectory/Exact.txt"), "content").unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();

        for relative in ["", "."] {
            let Resolution::Found { path, metadata } =
                resolve(&root, Path::new(relative), true, SymlinkPolicy::Deny).await
            else {
                panic!("the configured mount should resolve");
            };
            assert_eq!(path, root);
            assert!(metadata.is_dir());
        }

        let Resolution::Found { path, metadata } = resolve(
            &root,
            Path::new("./subdirectory/Exact.txt"),
            true,
            SymlinkPolicy::Deny,
        )
        .await
        else {
            panic!("an exactly spelled ordinary file should resolve");
        };
        assert_eq!(path, root.join("subdirectory/Exact.txt"));
        assert!(metadata.is_file());
        assert_eq!(metadata.len(), 7);
        assert!(matches!(
            resolve(&root, Path::new("missing"), true, SymlinkPolicy::Deny).await,
            Resolution::Missing
        ));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn strict_mode_rejects_case_aliases_of_ancestor_directories() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("ExactDirectory")).unwrap();
        fs::write(directory.path().join("ExactDirectory/file.txt"), "content").unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        let alias = Path::new("exactdirectory/file.txt");

        assert!(matches!(
            resolve(&root, alias, false, SymlinkPolicy::Deny).await,
            Resolution::Found { .. }
        ));
        assert!(matches!(
            resolve(&root, alias, true, SymlinkPolicy::Deny).await,
            Resolution::Rejected
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn link_policy_checks_ancestors_and_final_targets() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("target")).unwrap();
        fs::write(directory.path().join("target/file.txt"), "inside").unwrap();
        fs::write(outside.path().join("file.txt"), "outside").unwrap();
        symlink("target", directory.path().join("alias")).unwrap();
        symlink(outside.path(), directory.path().join("escape")).unwrap();
        symlink("target/file.txt", directory.path().join("file-link")).unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();

        for relative in ["alias/file.txt", "file-link", "escape/file.txt"] {
            assert!(matches!(
                resolve(&root, Path::new(relative), true, SymlinkPolicy::Deny).await,
                Resolution::Rejected
            ));
        }
        for relative in ["alias/file.txt", "file-link"] {
            let Resolution::Found { path, metadata } =
                resolve(&root, Path::new(relative), true, SymlinkPolicy::WithinRoot).await
            else {
                panic!("an exactly spelled link with an internal target should resolve");
            };
            assert_eq!(path, root.join("target/file.txt"));
            assert!(metadata.is_file());
            assert_eq!(metadata.len(), 6);
        }
        for strict in [false, true] {
            assert!(matches!(
                resolve(
                    &root,
                    Path::new("escape/file.txt"),
                    strict,
                    SymlinkPolicy::WithinRoot,
                )
                .await,
                Resolution::Rejected
            ));
        }
    }
}
