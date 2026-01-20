use pulldown_cmark::{html, Options, Parser};
use crate::files::FileTree;

const TEMPLATE: &str = include_str!("../../assets/template.html");
const TEMPLATE_SIDEBAR: &str = include_str!("../../assets/template_sidebar.html");
const CSS: &str = include_str!("../../assets/github.css");

pub struct HtmlRenderer {
    title: String,
}

impl HtmlRenderer {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
        }
    }

    /// Render markdown content to full HTML page (single file mode)
    pub fn render(&self, markdown: &str) -> String {
        let html_content = self.markdown_to_html(markdown);

        TEMPLATE
            .replace("{{TITLE}}", &self.title)
            .replace("{{CONTENT}}", &html_content)
    }

    /// Render markdown content with sidebar (directory mode)
    pub fn render_with_sidebar(
        &self,
        markdown: &str,
        file_tree: &FileTree,
        current_file: Option<&str>,
    ) -> String {
        let html_content = self.markdown_to_html(markdown);
        let sidebar_html = self.build_sidebar(file_tree, current_file);

        TEMPLATE_SIDEBAR
            .replace("{{TITLE}}", &self.title)
            .replace("{{SIDEBAR}}", &sidebar_html)
            .replace("{{CONTENT}}", &html_content)
    }

    /// Render only the content HTML (for AJAX loading)
    pub fn render_content(&self, markdown: &str) -> String {
        self.markdown_to_html(markdown)
    }

    /// Build sidebar HTML from file tree
    fn build_sidebar(&self, file_tree: &FileTree, current_file: Option<&str>) -> String {
        let mut html = String::new();

        // Group files by directory
        let mut dirs: std::collections::BTreeMap<String, Vec<&crate::files::MarkdownFile>> =
            std::collections::BTreeMap::new();

        for file in &file_tree.files {
            let parent = file
                .relative_path
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            dirs.entry(parent).or_default().push(file);
        }

        // Render file tree
        for (dir, files) in &dirs {
            if !dir.is_empty() {
                html.push_str(&format!(
                    r#"<div class="sidebar-folder">📁 {}</div>"#,
                    html_escape::encode_text(dir)
                ));
            }

            for file in files {
                let path = file.relative_path.to_string_lossy();
                let is_current = current_file.map_or(false, |c| c == path);
                let class = if is_current {
                    "sidebar-item active"
                } else {
                    "sidebar-item"
                };

                html.push_str(&format!(
                    r#"<a href="javascript:void(0)" class="{}" data-path="{}" onclick="loadFile('{}')">{}</a>"#,
                    class,
                    html_escape::encode_text(&path),
                    html_escape::encode_text(&path),
                    html_escape::encode_text(&file.name)
                ));
            }
        }

        html
    }

    /// Convert markdown to HTML fragment
    fn markdown_to_html(&self, markdown: &str) -> String {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);

        let parser = Parser::new_ext(markdown, options);
        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);

        // Convert relative .md links to use the viewer
        self.process_md_links(&html_output)
    }

    /// Process markdown links to make .md files open in the viewer
    fn process_md_links(&self, html: &str) -> String {
        // Simple regex-like replacement for .md links
        // Convert href="something.md" to onclick handler
        let mut result = html.to_string();

        // Find and replace .md links
        let link_pattern = regex::Regex::new(r#"href="([^"]+\.md)""#).ok();

        if let Some(re) = link_pattern {
            result = re
                .replace_all(&result, |caps: &regex::Captures| {
                    let path = &caps[1];
                    format!(
                        r#"href="javascript:void(0)" onclick="loadFile('{}')""#,
                        html_escape::encode_text(path)
                    )
                })
                .to_string();
        }

        result
    }

    /// Get CSS content for serving
    pub fn get_css() -> &'static str {
        CSS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_rendering() {
        let renderer = HtmlRenderer::new("Test");
        let result = renderer.render("# Hello\n\nWorld");
        assert!(result.contains("<h1>Hello</h1>"));
        assert!(result.contains("<p>World</p>"));
    }
}
