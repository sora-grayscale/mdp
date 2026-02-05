use std::collections::BTreeMap;
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

/// Represents a node in the file tree hierarchy
#[derive(Debug, Clone)]
pub enum TreeNode {
    File(MarkdownFile),
    Folder {
        name: String,
        children: Vec<TreeNode>,
    },
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
    /// Rejects paths containing ".." segments for security (path traversal prevention)
    ///
    /// Note: This check assumes the input is NOT URL-encoded. If URL-encoded paths
    /// (e.g., "%2e%2e" for "..") need to be handled, the caller should decode them first.
    pub fn find_file(&self, relative_path: &str) -> Option<&MarkdownFile> {
        // Security: reject paths with ".." as a path segment to prevent directory traversal
        // Check both / and \ as separators, also check start/end of string
        let normalized_for_check = relative_path.replace('\\', "/");
        let has_parent_ref = normalized_for_check
            .split('/')
            .any(|segment| segment == "..");
        if has_parent_ref {
            return None;
        }

        // Normalize input path: strip leading "./" and normalize separators
        let normalized_input = relative_path
            .trim_start_matches("./")
            .trim_start_matches(".\\")
            .replace('\\', "/");

        self.files.iter().find(|f| {
            let file_path = f.relative_path.to_string_lossy().replace('\\', "/");
            file_path == normalized_input
        })
    }

    /// Check if this is a single file (not directory mode)
    pub fn is_single_file(&self) -> bool {
        self.files.len() == 1
    }

    /// Build a hierarchical tree structure from the flat file list
    pub fn build_tree(&self) -> Vec<TreeNode> {
        // Use BTreeMap to maintain sorted order for folders
        // Key: folder path components, Value: nested structure
        let mut root_files: Vec<TreeNode> = Vec::new();
        let mut folders: BTreeMap<String, FolderBuilder> = BTreeMap::new();

        for file in &self.files {
            let components: Vec<&str> = file
                .relative_path
                .components()
                .filter_map(|c| c.as_os_str().to_str())
                .collect();

            if components.len() == 1 {
                // Root level file
                root_files.push(TreeNode::File(file.clone()));
            } else {
                // File in a subdirectory
                let folder_path = &components[..components.len() - 1];
                Self::insert_into_folder(&mut folders, folder_path, file.clone());
            }
        }

        // Build the final tree: root files first, then folders
        let mut result = root_files;
        for (name, builder) in folders {
            result.push(builder.into_tree_node(name));
        }

        result
    }

    /// Helper function to insert a file into the nested folder structure
    fn insert_into_folder(
        folders: &mut BTreeMap<String, FolderBuilder>,
        path: &[&str],
        file: MarkdownFile,
    ) {
        if path.is_empty() {
            return;
        }

        let first = path[0].to_string();
        let rest = &path[1..];

        let folder = folders.entry(first).or_insert_with(FolderBuilder::new);

        if rest.is_empty() {
            // File belongs directly in this folder
            folder.files.push(file);
        } else {
            // File belongs in a subfolder
            Self::insert_into_folder(&mut folder.subfolders, rest, file);
        }
    }
}

/// Helper struct for building folder hierarchy
#[derive(Debug)]
struct FolderBuilder {
    files: Vec<MarkdownFile>,
    subfolders: BTreeMap<String, FolderBuilder>,
}

impl FolderBuilder {
    fn new() -> Self {
        Self {
            files: Vec::new(),
            subfolders: BTreeMap::new(),
        }
    }

    fn into_tree_node(self, name: String) -> TreeNode {
        let mut children: Vec<TreeNode> = Vec::new();

        // Add files first
        for file in self.files {
            children.push(TreeNode::File(file));
        }

        // Then add subfolders
        for (subfolder_name, builder) in self.subfolders {
            children.push(builder.into_tree_node(subfolder_name));
        }

        TreeNode::Folder { name, children }
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

    #[test]
    fn test_build_tree_nested_directories() {
        let dir = tempdir().unwrap();

        // Create nested structure:
        // root.md
        // docs/
        //   guide.md
        //   api/
        //     reference.md
        //     advanced/
        //       deep.md
        let root = dir.path().join("root.md");
        let docs = dir.path().join("docs");
        let api = docs.join("api");
        let advanced = api.join("advanced");

        fs::create_dir_all(&advanced).unwrap();
        fs::write(&root, "# Root").unwrap();
        fs::write(docs.join("guide.md"), "# Guide").unwrap();
        fs::write(api.join("reference.md"), "# Reference").unwrap();
        fs::write(advanced.join("deep.md"), "# Deep").unwrap();

        let file_tree = FileTree::from_directory(dir.path()).unwrap();
        let tree = file_tree.build_tree();

        // Should have root file and docs folder at top level
        assert_eq!(tree.len(), 2);

        // First should be root.md (file)
        match &tree[0] {
            TreeNode::File(f) => assert_eq!(f.name, "root"),
            _ => panic!("Expected file at root level"),
        }

        // Second should be docs folder
        match &tree[1] {
            TreeNode::Folder { name, children } => {
                assert_eq!(name, "docs");
                assert_eq!(children.len(), 2); // guide.md and api folder

                // Check for api subfolder
                let api_folder = children.iter().find(|c| matches!(c, TreeNode::Folder { name, .. } if name == "api"));
                assert!(api_folder.is_some(), "Should have api subfolder");

                if let Some(TreeNode::Folder { children: api_children, .. }) = api_folder {
                    // api should contain reference.md and advanced folder
                    assert_eq!(api_children.len(), 2);

                    let advanced_folder = api_children.iter().find(|c| matches!(c, TreeNode::Folder { name, .. } if name == "advanced"));
                    assert!(advanced_folder.is_some(), "Should have advanced subfolder");
                }
            }
            _ => panic!("Expected folder at root level"),
        }
    }
}
