use pulldown_cmark::{html, Options, Parser};

const TEMPLATE: &str = include_str!("../../assets/template.html");
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

    /// Render markdown content to full HTML page
    pub fn render(&self, markdown: &str) -> String {
        let html_content = self.markdown_to_html(markdown);

        TEMPLATE
            .replace("{{TITLE}}", &self.title)
            .replace("{{CONTENT}}", &html_content)
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
        html_output
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
