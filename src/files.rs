use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Represents a markdown file with its relative path
#[derive(Debug, Clone)]
pub struct MarkdownFile {
    /// Absolute path to the file
    pub absolute_path: PathBuf,
    /// Relative path from the base directory
    pub relative_path: PathBuf,
    /// Display name (filename without extension)
    pub name: String,
}

/// Represents a directory structure of markdown files
#[derive(Debug, Clone)]
pub struct FileTree {
    /// Base directory path
    pub base_path: PathBuf,
    /// All markdown files found
    pub files: Vec<MarkdownFile>,
}

impl FileTree {
    /// Create a FileTree from a directory path
    pub fn from_directory(path: &Path) -> std::io::Result<Self> {
        let base_path = path.canonicalize()?;
        let mut files = Vec::new();

        // Don't follow symlinks to avoid infinite loops with circular symlinks
        for entry in WalkDir::new(&base_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let entry_path = entry.path();

            // Skip directories and non-markdown files
            if entry_path.is_dir() {
                continue;
            }

            if let Some(ext) = entry_path.extension()
                && (ext == "md" || ext == "markdown")
            {
                let relative_path = entry_path
                    .strip_prefix(&base_path)
                    .unwrap_or(entry_path)
                    .to_path_buf();

                let name = entry_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("untitled")
                    .to_string();

                files.push(MarkdownFile {
                    absolute_path: entry_path.to_path_buf(),
                    relative_path,
                    name,
                });
            }
        }

        // Sort files: README first, then alphabetically
        files.sort_by(|a, b| {
            let a_is_readme = a.name.to_lowercase() == "readme";
            let b_is_readme = b.name.to_lowercase() == "readme";

            match (a_is_readme, b_is_readme) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.relative_path.cmp(&b.relative_path),
            }
        });

        Ok(FileTree { base_path, files })
    }

    /// Create a FileTree from a single file
    pub fn from_file(path: &Path) -> std::io::Result<Self> {
        let absolute_path = path.canonicalize()?;
        let base_path = absolute_path
            .parent()
            .unwrap_or(&absolute_path)
            .to_path_buf();

        let relative_path = absolute_path
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| absolute_path.clone());

        let name = absolute_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_string();

        let files = vec![MarkdownFile {
            absolute_path,
            relative_path,
            name,
        }];

        Ok(FileTree { base_path, files })
    }

    /// Create a FileTree from a file with context (sibling/child markdown files)
    /// This scans the file's parent directory recursively for related markdown files
    pub fn from_file_with_context(path: &Path) -> std::io::Result<Self> {
        let absolute_path = path.canonicalize()?;
        let base_path = absolute_path
            .parent()
            .unwrap_or(&absolute_path)
            .to_path_buf();

        // Use from_directory to get all markdown files in the parent directory
        let mut tree = Self::from_directory(&base_path)?;

        // Ensure the specified file is the default (first in list)
        let target_relative = absolute_path
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| absolute_path.clone());

        // Re-sort: specified file first, then README, then alphabetically
        tree.files.sort_by(|a, b| {
            let a_is_target = a.relative_path == target_relative;
            let b_is_target = b.relative_path == target_relative;
            let a_is_readme = a.name.to_lowercase() == "readme";
            let b_is_readme = b.name.to_lowercase() == "readme";

            match (a_is_target, b_is_target) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => match (a_is_readme, b_is_readme) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.relative_path.cmp(&b.relative_path),
                },
            }
        });

        Ok(tree)
    }

    /// Get the default file to display (README or first file)
    pub fn default_file(&self) -> Option<&MarkdownFile> {
        self.files.first()
    }

    /// Find a file by its relative path
    /// Normalizes the path to handle cases like "./a.md" vs "a.md"
    pub fn find_file(&self, relative_path: &str) -> Option<&MarkdownFile> {
        // Normalize input path: strip leading "./" and normalize separators
        let normalized_input = relative_path
            .trim_start_matches("./")
            .trim_start_matches(".\\")
            .replace('\\', "/");

        self.files.iter().find(|f| {
            let file_path = f
                .relative_path
                .to_string_lossy()
                .replace('\\', "/");
            file_path == normalized_input
        })
    }

    /// Check if this is a single file (not directory mode)
    pub fn is_single_file(&self) -> bool {
        self.files.len() == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_from_directory() {
        let dir = tempdir().unwrap();
        let readme = dir.path().join("README.md");
        let guide = dir.path().join("guide.md");
        let subdir = dir.path().join("docs");
        fs::create_dir(&subdir).unwrap();
        let api = subdir.join("api.md");

        fs::write(&readme, "# README").unwrap();
        fs::write(&guide, "# Guide").unwrap();
        fs::write(&api, "# API").unwrap();

        let tree = FileTree::from_directory(dir.path()).unwrap();

        assert_eq!(tree.files.len(), 3);
        // README should be first
        assert_eq!(tree.files[0].name, "README");
    }
}
