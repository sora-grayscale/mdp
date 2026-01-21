use crate::files::FileTree;
use crate::parser::AnchorGenerator;
use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};

const TEMPLATE: &str = include_str!("../../assets/template.html");
const TEMPLATE_SIDEBAR: &str = include_str!("../../assets/template_sidebar.html");
const CSS: &str = include_str!("../../assets/github.css");

// SVG icons for the sidebar
const ICON_FILE: &str = r#"<svg class="sidebar-item-icon" viewBox="0 0 16 16"><path d="M2 1.75C2 .784 2.784 0 3.75 0h6.586c.464 0 .909.184 1.237.513l2.914 2.914c.329.328.513.773.513 1.237v9.586A1.75 1.75 0 0 1 13.25 16h-9.5A1.75 1.75 0 0 1 2 14.25Zm1.75-.25a.25.25 0 0 0-.25.25v12.5c0 .138.112.25.25.25h9.5a.25.25 0 0 0 .25-.25V6h-2.75A1.75 1.75 0 0 1 9 4.25V1.5Zm6.75.062V4.25c0 .138.112.25.25.25h2.688l-.011-.013-2.914-2.914-.013-.011Z"/></svg>"#;
const ICON_CHEVRON: &str = r#"<svg class="sidebar-folder-icon" viewBox="0 0 16 16"><path d="M12.78 5.22a.749.749 0 0 1 0 1.06l-4.25 4.25a.749.749 0 0 1-1.06 0L3.22 6.28a.749.749 0 1 1 1.06-1.06L8 8.939l3.72-3.719a.749.749 0 0 1 1.06 0Z"/></svg>"#;

pub struct HtmlRenderer {
    title: String,
    show_toc: bool,
}

impl HtmlRenderer {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            show_toc: false,
        }
    }

    pub fn with_toc(mut self, show_toc: bool) -> Self {
        self.show_toc = show_toc;
        self
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
            if dir.is_empty() {
                // Root level files
                for file in files {
                    html.push_str(&self.render_file_item(file, current_file, true));
                }
            } else {
                // Files in a folder
                let folder_id = dir.replace(['/', '\\'], "_");
                html.push_str(&format!(
                    r#"<div class="sidebar-folder" data-folder="{}">
                        <div class="sidebar-folder-header" onclick="toggleFolder('{}')">
                            {}
                            <span class="sidebar-folder-name">{}</span>
                        </div>
                        <div class="sidebar-folder-items">"#,
                    html_escape::encode_text(&folder_id),
                    html_escape::encode_text(&folder_id),
                    ICON_CHEVRON,
                    html_escape::encode_text(dir)
                ));

                for file in files {
                    html.push_str(&self.render_file_item(file, current_file, false));
                }

                html.push_str("</div></div>");
            }
        }

        html
    }

    /// Render a single file item in the sidebar
    fn render_file_item(
        &self,
        file: &crate::files::MarkdownFile,
        current_file: Option<&str>,
        is_root: bool,
    ) -> String {
        let path = file.relative_path.to_string_lossy();
        let is_current = current_file.is_some_and(|c| c == path);

        let mut classes = vec!["sidebar-item"];
        if is_current {
            classes.push("active");
        }
        if is_root {
            classes.push("root-item");
        }

        format!(
            r#"<a href="javascript:void(0)" class="{}" data-path="{}" onclick="loadFile('{}')">
                {}
                <span class="sidebar-item-name">{}</span>
            </a>"#,
            classes.join(" "),
            html_escape::encode_text(&path),
            html_escape::encode_text(&path),
            ICON_FILE,
            html_escape::encode_text(&file.name)
        )
    }

    /// Convert markdown to HTML fragment
    fn markdown_to_html(&self, markdown: &str) -> String {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_FOOTNOTES);

        let parser = Parser::new_ext(markdown, options);

        // Collect TOC entries and add IDs to headings
        let mut toc_entries: Vec<(u8, String, String)> = Vec::new(); // (level, text, anchor)
        let mut anchor_gen = AnchorGenerator::new();
        let mut main_events: Vec<Event> = Vec::new();
        let mut footnote_events: Vec<Event> = Vec::new();
        let mut in_footnote = false;
        let mut in_heading = false;
        let mut current_heading_level: u8 = 0;
        let mut current_heading_text = String::new();

        for event in parser {
            match &event {
                Event::Start(Tag::FootnoteDefinition(_)) => {
                    in_footnote = true;
                    footnote_events.push(event);
                }
                Event::End(TagEnd::FootnoteDefinition) => {
                    footnote_events.push(event);
                    in_footnote = false;
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    in_heading = true;
                    current_heading_level = Self::heading_level_to_u8(*level);
                    current_heading_text.clear();
                    // Don't push yet, we'll create a new event with id
                }
                Event::End(TagEnd::Heading(_)) => {
                    in_heading = false;

                    // Generate anchor using shared utility
                    let anchor = anchor_gen.generate(&current_heading_text);

                    // Store TOC entry
                    toc_entries.push((
                        current_heading_level,
                        current_heading_text.clone(),
                        anchor.clone(),
                    ));

                    // Create heading with id attribute
                    let level = match current_heading_level {
                        1 => HeadingLevel::H1,
                        2 => HeadingLevel::H2,
                        3 => HeadingLevel::H3,
                        4 => HeadingLevel::H4,
                        5 => HeadingLevel::H5,
                        _ => HeadingLevel::H6,
                    };
                    main_events.push(Event::Start(Tag::Heading {
                        level,
                        id: Some(CowStr::Boxed(anchor.into_boxed_str())),
                        classes: vec![],
                        attrs: vec![],
                    }));
                    main_events.push(Event::Text(CowStr::Boxed(
                        current_heading_text.clone().into_boxed_str(),
                    )));
                    main_events.push(event);
                }
                Event::Text(text) if in_heading => {
                    current_heading_text.push_str(text);
                }
                Event::Code(code) if in_heading => {
                    current_heading_text.push_str(code);
                }
                _ => {
                    if in_footnote {
                        footnote_events.push(event);
                    } else if !in_heading {
                        main_events.push(event);
                    }
                }
            }
        }

        // Build TOC HTML if enabled
        let mut html_output = String::new();
        if self.show_toc && !toc_entries.is_empty() {
            html_output.push_str("<nav class=\"toc\">\n");
            html_output.push_str("<h2>📑 Table of Contents</h2>\n");
            html_output.push_str("<ul>\n");

            let min_level = toc_entries.iter().map(|(l, _, _)| *l).min().unwrap_or(1);
            for (level, text, anchor) in &toc_entries {
                let indent = "  ".repeat((*level - min_level) as usize);
                html_output.push_str(&format!(
                    "{}<li><a href=\"#{}\">{}</a></li>\n",
                    indent,
                    html_escape::encode_text(anchor),
                    html_escape::encode_text(text)
                ));
            }

            html_output.push_str("</ul>\n");
            html_output.push_str("</nav>\n");
            html_output.push_str("<hr />\n");
        }

        // Render main content
        html::push_html(&mut html_output, main_events.into_iter());

        // Render footnotes at the end with separator
        if !footnote_events.is_empty() {
            html_output.push_str("<hr class=\"footnotes-separator\" />\n");
            html_output.push_str("<section class=\"footnotes\">\n");
            html::push_html(&mut html_output, footnote_events.into_iter());
            html_output.push_str("</section>\n");
        }

        // Process links
        self.process_links(&html_output)
    }

    fn heading_level_to_u8(level: HeadingLevel) -> u8 {
        match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 => 5,
            HeadingLevel::H6 => 6,
        }
    }

    /// Process links in HTML
    /// - Convert .md links to use the viewer
    /// - Add target="_blank" to external links
    fn process_links(&self, html: &str) -> String {
        let mut result = html.to_string();

        // Pattern for all href links
        let link_pattern = regex::Regex::new(r#"<a\s+href="([^"]+)"([^>]*)>"#).ok();

        if let Some(re) = link_pattern {
            result = re
                .replace_all(&result, |caps: &regex::Captures| {
                    let url = &caps[1];
                    let rest = &caps[2];

                    if url.starts_with("http://") || url.starts_with("https://") {
                        // External link - open in new tab
                        format!(
                            r#"<a href="{}" target="_blank" rel="noopener noreferrer"{}>"#,
                            url, rest
                        )
                    } else if url.ends_with(".md") {
                        // Local .md file - use viewer
                        format!(
                            r#"<a href="javascript:void(0)" onclick="loadFile('{}')"{}>"#,
                            html_escape::encode_text(url),
                            rest
                        )
                    } else {
                        // Other links - keep as is
                        format!(r#"<a href="{}"{}>"#, url, rest)
                    }
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
        assert!(result.contains("<h1 id=\"hello\">Hello</h1>"));
        assert!(result.contains("<p>World</p>"));
    }

    #[test]
    fn test_external_links() {
        let renderer = HtmlRenderer::new("Test");
        let result = renderer.render("[Google](https://google.com)");
        assert!(result.contains(r#"target="_blank""#));
        assert!(result.contains(r#"rel="noopener noreferrer""#));
    }

    #[test]
    fn test_md_links() {
        let renderer = HtmlRenderer::new("Test");
        let result = renderer.render("[Guide](./guide.md)");
        assert!(result.contains(r#"onclick="loadFile"#));
    }
}
