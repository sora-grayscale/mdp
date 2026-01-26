use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Represents a parsed Markdown document
#[derive(Debug, Clone)]
pub struct Document {
    pub elements: Vec<Element>,
}

/// Represents a single element in the document
#[derive(Debug, Clone)]
pub enum Element {
    Heading {
        level: u8,
        content: String,
    },
    Paragraph {
        content: Vec<InlineElement>,
    },
    CodeBlock {
        language: Option<String>,
        content: String,
    },
    List {
        ordered: bool,
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Table {
        headers: Vec<String>,
        alignments: Vec<Alignment>,
        rows: Vec<Vec<String>>,
    },
    BlockQuote {
        content: Vec<Element>,
    },
    HorizontalRule,
    Image {
        url: String,
        alt: String,
        title: Option<String>,
    },
    FootnoteDefinition {
        label: String,
        content: Vec<Element>,
    },
    /// Raw HTML block
    Html(String),
}

/// A list item containing zero or more block elements
/// Per GFM spec, list items can contain paragraphs, code blocks, nested lists, etc.
#[derive(Debug, Clone)]
pub struct ListItem {
    pub content: Vec<Element>,
}

#[derive(Debug, Clone)]
pub enum InlineElement {
    Text(String),
    Code(String),
    Strong(Vec<InlineElement>),
    Emphasis(Vec<InlineElement>),
    Strikethrough(Vec<InlineElement>),
    Link {
        url: String,
        content: Vec<InlineElement>,
        title: Option<String>,
    },
    Image {
        url: String,
        alt: String,
        title: Option<String>,
    },
    FootnoteReference(String),
    /// Task list checkbox (true = checked)
    TaskListMarker(bool),
    /// Inline HTML (e.g., <br>, <span>)
    InlineHtml(String),
    SoftBreak,
    HardBreak,
}

#[derive(Debug, Clone, Copy)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

impl From<pulldown_cmark::Alignment> for Alignment {
    fn from(align: pulldown_cmark::Alignment) -> Self {
        match align {
            pulldown_cmark::Alignment::None => Alignment::None,
            pulldown_cmark::Alignment::Left => Alignment::Left,
            pulldown_cmark::Alignment::Center => Alignment::Center,
            pulldown_cmark::Alignment::Right => Alignment::Right,
        }
    }
}

/// Entry in the table of contents
#[derive(Debug, Clone)]
pub struct TocEntry {
    pub level: u8,
    pub text: String,
    pub anchor: String,
}

/// Generate an anchor slug from heading text
pub fn generate_anchor(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// Manages anchor generation with duplicate handling
#[derive(Debug, Default)]
pub struct AnchorGenerator {
    counts: std::collections::HashMap<String, usize>,
}

impl AnchorGenerator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate a unique anchor from text, handling duplicates
    pub fn generate(&mut self, text: &str) -> String {
        let base_anchor = generate_anchor(text);

        let anchor = if let Some(count) = self.counts.get(&base_anchor) {
            format!("{}-{}", base_anchor, count)
        } else {
            base_anchor.clone()
        };

        *self.counts.entry(base_anchor).or_insert(0) += 1;
        anchor
    }
}

/// Generate table of contents from a document
pub fn generate_toc(document: &Document) -> Vec<TocEntry> {
    let mut entries = Vec::new();
    let mut anchor_gen = AnchorGenerator::new();

    for element in &document.elements {
        if let Element::Heading { level, content } = element {
            let anchor = anchor_gen.generate(content);

            entries.push(TocEntry {
                level: *level,
                text: content.clone(),
                anchor,
            });
        }
    }

    entries
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

/// Parse a Markdown string into a Document
pub fn parse_markdown(input: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(input, options);
    let events: Vec<Event> = parser.collect();

    let mut elements = Vec::new();
    let mut index = 0;

    while index < events.len() {
        let (element, new_index) = parse_element(&events, index);
        if let Some(el) = element {
            elements.push(el);
        }
        index = new_index;
    }

    Document { elements }
}

/// Helper to compare TagEnd variants properly (handles variants with data)
/// Check if two TagEnd variants match (for inline element parsing)
/// Only includes variants actually used as end_tag in parse_inline_elements:
/// - Inline elements: Emphasis, Strong, Strikethrough, Link, Image
/// - Block boundaries containing inline content: Paragraph, Item
fn tag_end_matches(actual: &TagEnd, expected: &TagEnd) -> bool {
    matches!(
        (actual, expected),
        // Inline elements
        (TagEnd::Emphasis, TagEnd::Emphasis)
            | (TagEnd::Strong, TagEnd::Strong)
            | (TagEnd::Strikethrough, TagEnd::Strikethrough)
            | (TagEnd::Link, TagEnd::Link)
            | (TagEnd::Image, TagEnd::Image)
            // Block boundaries that contain inline content
            | (TagEnd::Paragraph, TagEnd::Paragraph)
            | (TagEnd::Item, TagEnd::Item)
    )
}

/// Parse inline elements recursively, handling nested structures like **[link](url)**
fn parse_inline_elements(
    events: &[Event],
    start: usize,
    end_tag: Option<TagEnd>,
) -> (Vec<InlineElement>, usize) {
    let mut elements = Vec::new();
    let mut index = start;

    while index < events.len() {
        // Check if we hit our expected end tag (for inline elements like Strong, Emphasis, etc.)
        if let Some(ref end) = end_tag {
            if let Event::End(tag_end) = &events[index] {
                if tag_end_matches(tag_end, end) {
                    return (elements, index);
                }
            }
        }

        match &events[index] {
            // Block-level end tags: only terminate when we have no specific end_tag
            // (i.e., we're parsing top-level inline content within a block)
            // When end_tag is Some (parsing nested inline), we skip these and let parent handle
            Event::End(TagEnd::Paragraph)
            | Event::End(TagEnd::Item)
            | Event::End(TagEnd::BlockQuote)
            | Event::End(TagEnd::FootnoteDefinition) => {
                if end_tag.is_none() {
                    // Top-level parsing, this is our boundary
                    return (elements, index);
                }
                // Inside nested inline element - skip and continue
                // This shouldn't happen in well-formed markdown, but handle gracefully
            }

            Event::Text(text) => {
                elements.push(InlineElement::Text(text.to_string()));
            }

            Event::Code(code) => {
                elements.push(InlineElement::Code(code.to_string()));
            }

            Event::Start(Tag::Strong) => {
                let (content, new_index) =
                    parse_inline_elements(events, index + 1, Some(TagEnd::Strong));
                elements.push(InlineElement::Strong(content));
                index = new_index;
            }

            Event::Start(Tag::Emphasis) => {
                let (content, new_index) =
                    parse_inline_elements(events, index + 1, Some(TagEnd::Emphasis));
                elements.push(InlineElement::Emphasis(content));
                index = new_index;
            }

            Event::Start(Tag::Strikethrough) => {
                let (content, new_index) =
                    parse_inline_elements(events, index + 1, Some(TagEnd::Strikethrough));
                elements.push(InlineElement::Strikethrough(content));
                index = new_index;
            }

            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                let url = dest_url.to_string();
                let title = if title.is_empty() {
                    None
                } else {
                    Some(title.to_string())
                };
                let (content, new_index) =
                    parse_inline_elements(events, index + 1, Some(TagEnd::Link));
                elements.push(InlineElement::Link {
                    url,
                    content,
                    title,
                });
                index = new_index;
            }

            Event::FootnoteReference(label) => {
                elements.push(InlineElement::FootnoteReference(label.to_string()));
            }

            Event::SoftBreak => {
                elements.push(InlineElement::SoftBreak);
            }

            Event::HardBreak => {
                elements.push(InlineElement::HardBreak);
            }

            Event::TaskListMarker(checked) => {
                elements.push(InlineElement::TaskListMarker(*checked));
            }

            Event::InlineHtml(html) => {
                elements.push(InlineElement::InlineHtml(html.to_string()));
            }

            Event::Start(Tag::Image {
                link_type: _,
                dest_url,
                title,
                id: _,
            }) => {
                let url = dest_url.to_string();
                let title = if title.is_empty() {
                    None
                } else {
                    Some(title.to_string())
                };
                // Collect alt text from events until End(Image)
                let mut alt = String::new();
                index += 1;
                while index < events.len() {
                    match &events[index] {
                        Event::End(TagEnd::Image) => {
                            break;
                        }
                        Event::Text(text) => {
                            alt.push_str(text);
                        }
                        _ => {}
                    }
                    index += 1;
                }
                elements.push(InlineElement::Image { url, alt, title });
            }

            // Skip other events (nested block elements are handled by parse_element)
            _ => {}
        }

        index += 1;
    }

    (elements, index)
}

fn parse_element(events: &[Event], start: usize) -> (Option<Element>, usize) {
    if start >= events.len() {
        return (None, start + 1);
    }

    match &events[start] {
        Event::Start(Tag::Heading { level, .. }) => {
            let level = heading_level_to_u8(*level);
            let mut content = String::new();
            let mut index = start + 1;

            while index < events.len() {
                match &events[index] {
                    Event::End(TagEnd::Heading(_)) => {
                        break;
                    }
                    Event::Text(text) | Event::Code(text) => {
                        content.push_str(text);
                    }
                    _ => {}
                }
                index += 1;
            }

            (Some(Element::Heading { level, content }), index + 1)
        }

        Event::Start(Tag::Paragraph) => {
            let (inline_elements, end_index) =
                parse_inline_elements(events, start + 1, Some(TagEnd::Paragraph));
            (
                Some(Element::Paragraph {
                    content: inline_elements,
                }),
                end_index + 1,
            )
        }

        Event::Start(Tag::CodeBlock(kind)) => {
            let language = match kind {
                CodeBlockKind::Fenced(lang) => {
                    if lang.is_empty() {
                        None
                    } else {
                        Some(lang.to_string())
                    }
                }
                CodeBlockKind::Indented => None,
            };

            let mut content = String::new();
            let mut index = start + 1;

            while index < events.len() {
                match &events[index] {
                    Event::End(TagEnd::CodeBlock) => {
                        break;
                    }
                    Event::Text(text) => {
                        content.push_str(text);
                    }
                    _ => {}
                }
                index += 1;
            }

            (Some(Element::CodeBlock { language, content }), index + 1)
        }

        Event::Start(Tag::List(first_item_number)) => {
            let ordered = first_item_number.is_some();
            let start_num = *first_item_number;
            let mut items = Vec::new();
            let mut index = start + 1;

            while index < events.len() {
                match &events[index] {
                    Event::End(TagEnd::List(_)) => {
                        break;
                    }
                    Event::Start(Tag::Item) => {
                        let mut item_content: Vec<Element> = Vec::new();
                        index += 1;

                        // Parse block elements within the list item
                        while index < events.len() {
                            match &events[index] {
                                Event::End(TagEnd::Item) => {
                                    break;
                                }
                                // Block elements: parse recursively
                                Event::Start(Tag::List(_))
                                | Event::Start(Tag::Paragraph)
                                | Event::Start(Tag::CodeBlock(_))
                                | Event::Start(Tag::BlockQuote)
                                | Event::Start(Tag::Table(_)) => {
                                    let (element, new_index) = parse_element(events, index);
                                    if let Some(el) = element {
                                        item_content.push(el);
                                    }
                                    index = new_index;
                                    continue;
                                }
                                // Loose inline content (text without paragraph wrapper)
                                // Wrap in a paragraph for consistency
                                Event::Text(_)
                                | Event::Code(_)
                                | Event::TaskListMarker(_)
                                | Event::Start(Tag::Strong)
                                | Event::Start(Tag::Emphasis)
                                | Event::Start(Tag::Strikethrough)
                                | Event::Start(Tag::Link { .. }) => {
                                    let (inline_content, new_index) =
                                        parse_inline_elements(events, index, Some(TagEnd::Item));
                                    if !inline_content.is_empty() {
                                        item_content.push(Element::Paragraph {
                                            content: inline_content,
                                        });
                                    }
                                    index = new_index;
                                    continue;
                                }
                                _ => {
                                    index += 1;
                                    continue;
                                }
                            }
                        }

                        items.push(ListItem {
                            content: item_content,
                        });
                    }
                    _ => {}
                }
                index += 1;
            }

            (
                Some(Element::List {
                    ordered,
                    start: start_num,
                    items,
                }),
                index + 1,
            )
        }

        Event::Start(Tag::Table(alignments)) => {
            let alignments: Vec<Alignment> = alignments.iter().map(|a| (*a).into()).collect();
            let mut headers = Vec::new();
            let mut rows = Vec::new();
            let mut index = start + 1;
            let mut current_row = Vec::new();
            let mut current_cell = String::new();

            while index < events.len() {
                match &events[index] {
                    Event::End(TagEnd::Table) => {
                        break;
                    }
                    Event::Start(Tag::TableHead) => {
                        current_row = Vec::new();
                    }
                    Event::End(TagEnd::TableHead) => {
                        // TableHead contains cells directly without TableRow in pulldown-cmark 0.10
                        headers = current_row.clone();
                    }
                    Event::Start(Tag::TableRow) => {
                        current_row = Vec::new();
                    }
                    Event::End(TagEnd::TableRow) => {
                        rows.push(current_row.clone());
                    }
                    Event::Start(Tag::TableCell) => {
                        current_cell = String::new();
                    }
                    Event::End(TagEnd::TableCell) => {
                        current_row.push(current_cell.clone());
                    }
                    Event::Text(text) => {
                        current_cell.push_str(text);
                    }
                    Event::Code(code) => {
                        current_cell.push_str(&format!("`{}`", code));
                    }
                    _ => {}
                }
                index += 1;
            }

            (
                Some(Element::Table {
                    headers,
                    alignments,
                    rows,
                }),
                index + 1,
            )
        }

        Event::Start(Tag::BlockQuote) => {
            let mut content = Vec::new();
            let mut index = start + 1;
            let mut depth = 1;

            while index < events.len() {
                match &events[index] {
                    Event::End(TagEnd::BlockQuote) => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    Event::Start(Tag::BlockQuote) => {
                        depth += 1;
                    }
                    _ => {
                        let (element, new_index) = parse_element(events, index);
                        if let Some(el) = element {
                            content.push(el);
                        }
                        index = new_index - 1;
                    }
                }
                index += 1;
            }

            (Some(Element::BlockQuote { content }), index + 1)
        }

        Event::Rule => (Some(Element::HorizontalRule), start + 1),

        Event::Start(Tag::Image {
            link_type: _,
            dest_url,
            title,
            id: _,
        }) => {
            let url = dest_url.to_string();
            let title = if title.is_empty() {
                None
            } else {
                Some(title.to_string())
            };
            let mut alt = String::new();
            let mut index = start + 1;

            while index < events.len() {
                match &events[index] {
                    Event::End(TagEnd::Image) => {
                        break;
                    }
                    Event::Text(text) => {
                        alt.push_str(text);
                    }
                    _ => {}
                }
                index += 1;
            }

            (Some(Element::Image { url, alt, title }), index + 1)
        }

        Event::Start(Tag::FootnoteDefinition(label)) => {
            let label = label.to_string();
            let mut content = Vec::new();
            let mut index = start + 1;

            while index < events.len() {
                match &events[index] {
                    Event::End(TagEnd::FootnoteDefinition) => {
                        break;
                    }
                    _ => {
                        let (element, new_index) = parse_element(events, index);
                        if let Some(el) = element {
                            content.push(el);
                        }
                        index = new_index - 1;
                    }
                }
                index += 1;
            }

            (
                Some(Element::FootnoteDefinition { label, content }),
                index + 1,
            )
        }

        Event::Html(html) => (Some(Element::Html(html.to_string())), start + 1),

        _ => (None, start + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_footnote_reference() {
        let input = "This has a footnote[^1].\n\n[^1]: The footnote content.";
        let doc = parse_markdown(input);

        // Should have a paragraph with footnote reference and a footnote definition
        assert!(doc.elements.len() >= 2);

        // Check the paragraph contains a footnote reference
        if let Element::Paragraph { content } = &doc.elements[0] {
            let has_footnote_ref = content
                .iter()
                .any(|el| matches!(el, InlineElement::FootnoteReference(label) if label == "1"));
            assert!(has_footnote_ref, "Should have footnote reference");
        } else {
            panic!("First element should be a paragraph");
        }

        // Check footnote definition exists
        let has_footnote_def = doc
            .elements
            .iter()
            .any(|el| matches!(el, Element::FootnoteDefinition { label, .. } if label == "1"));
        assert!(has_footnote_def, "Should have footnote definition");
    }

    #[test]
    fn test_footnote_definition_content() {
        let input = "[^note]: This is the **footnote** content.";
        let doc = parse_markdown(input);

        // Find the footnote definition
        let footnote = doc.elements.iter().find_map(|el| {
            if let Element::FootnoteDefinition { label, content } = el {
                if label == "note" {
                    return Some(content);
                }
            }
            None
        });

        assert!(footnote.is_some(), "Should have footnote definition");
        let content = footnote.unwrap();
        assert!(!content.is_empty(), "Footnote should have content");
    }

    #[test]
    fn test_generate_anchor() {
        assert_eq!(generate_anchor("Hello World"), "hello-world");
        assert_eq!(generate_anchor("Hello, World!"), "hello-world");
        assert_eq!(generate_anchor("Test 123"), "test-123");
        assert_eq!(generate_anchor("CamelCase"), "camelcase");
        assert_eq!(generate_anchor("multiple   spaces"), "multiple-spaces");
    }

    #[test]
    fn test_anchor_generator_duplicates() {
        let mut anchor_gen = AnchorGenerator::new();
        assert_eq!(anchor_gen.generate("Hello"), "hello");
        assert_eq!(anchor_gen.generate("Hello"), "hello-1");
        assert_eq!(anchor_gen.generate("Hello"), "hello-2");
        assert_eq!(anchor_gen.generate("World"), "world");
        assert_eq!(anchor_gen.generate("Hello"), "hello-3");
    }

    #[test]
    fn test_nested_strong_emphasis() {
        let input = "This is **bold with _italic_ inside** text.";
        let doc = parse_markdown(input);

        if let Element::Paragraph { content } = &doc.elements[0] {
            // Should have: Text, Strong(with nested Emphasis), Text
            let has_nested = content.iter().any(|el| {
                if let InlineElement::Strong(inner) = el {
                    inner
                        .iter()
                        .any(|i| matches!(i, InlineElement::Emphasis(_)))
                } else {
                    false
                }
            });
            assert!(has_nested, "Should have Strong containing Emphasis");
        } else {
            panic!("First element should be a paragraph");
        }
    }

    #[test]
    fn test_link_with_nested_elements() {
        let input = "Check out [**bold link**](https://example.com)!";
        let doc = parse_markdown(input);

        if let Element::Paragraph { content } = &doc.elements[0] {
            let has_bold_link = content.iter().any(|el| {
                if let InlineElement::Link { content, url, .. } = el {
                    url == "https://example.com"
                        && content
                            .iter()
                            .any(|i| matches!(i, InlineElement::Strong(_)))
                } else {
                    false
                }
            });
            assert!(has_bold_link, "Should have Link containing Strong");
        } else {
            panic!("First element should be a paragraph");
        }
    }

    #[test]
    fn test_list_with_nested_inline() {
        let input = "- Item with **bold** text\n- Another _italic_ item";
        let doc = parse_markdown(input);

        if let Element::List { items, .. } = &doc.elements[0] {
            assert_eq!(items.len(), 2);

            // Helper to check if Element::Paragraph contains a specific inline element
            fn paragraph_contains<F>(elements: &[Element], predicate: F) -> bool
            where
                F: Fn(&InlineElement) -> bool,
            {
                elements.iter().any(|el| {
                    if let Element::Paragraph { content } = el {
                        content.iter().any(&predicate)
                    } else {
                        false
                    }
                })
            }

            // First item should have Strong
            let first_has_strong = paragraph_contains(&items[0].content, |el| {
                matches!(el, InlineElement::Strong(_))
            });
            assert!(first_has_strong, "First item should have Strong");

            // Second item should have Emphasis
            let second_has_emphasis = paragraph_contains(&items[1].content, |el| {
                matches!(el, InlineElement::Emphasis(_))
            });
            assert!(second_has_emphasis, "Second item should have Emphasis");
        } else {
            panic!("First element should be a list");
        }
    }

    #[test]
    fn test_list_with_code_block() {
        let input = "- Item with code:\n\n  ```rust\n  let x = 1;\n  ```\n\n- Another item";
        let doc = parse_markdown(input);

        if let Element::List { items, .. } = &doc.elements[0] {
            // First item should contain a code block
            let first_has_code = items[0]
                .content
                .iter()
                .any(|el| matches!(el, Element::CodeBlock { .. }));
            assert!(first_has_code, "First item should have CodeBlock");
        } else {
            panic!("First element should be a list");
        }
    }

    #[test]
    fn test_task_list() {
        let input = "- [ ] Unchecked\n- [x] Checked";
        let doc = parse_markdown(input);

        if let Element::List { items, .. } = &doc.elements[0] {
            assert_eq!(items.len(), 2);

            // Helper to find TaskListMarker in item content
            fn find_task_marker(elements: &[Element]) -> Option<bool> {
                for el in elements {
                    if let Element::Paragraph { content } = el {
                        for inline in content {
                            if let InlineElement::TaskListMarker(checked) = inline {
                                return Some(*checked);
                            }
                        }
                    }
                }
                None
            }

            // First item should have unchecked marker
            assert_eq!(
                find_task_marker(&items[0].content),
                Some(false),
                "First item should have unchecked marker"
            );

            // Second item should have checked marker
            assert_eq!(
                find_task_marker(&items[1].content),
                Some(true),
                "Second item should have checked marker"
            );
        } else {
            panic!("First element should be a list");
        }
    }

    #[test]
    fn test_inline_image() {
        let input = "Here is ![alt text](https://example.com/img.png \"title\") inline.";
        let doc = parse_markdown(input);

        if let Element::Paragraph { content } = &doc.elements[0] {
            let has_image = content.iter().any(|el| {
                matches!(
                    el,
                    InlineElement::Image {
                        url,
                        alt,
                        title: Some(_)
                    } if url == "https://example.com/img.png" && alt == "alt text"
                )
            });
            assert!(has_image, "Should have inline image");
        } else {
            panic!("First element should be a paragraph");
        }
    }

    #[test]
    fn test_inline_html() {
        let input = "Text with <br> and <span>content</span>.";
        let doc = parse_markdown(input);

        if let Element::Paragraph { content } = &doc.elements[0] {
            let has_inline_html = content
                .iter()
                .any(|el| matches!(el, InlineElement::InlineHtml(_)));
            assert!(has_inline_html, "Should have inline HTML");
        } else {
            panic!("First element should be a paragraph");
        }
    }

    #[test]
    fn test_html_block() {
        let input = "<div>\n  <p>HTML block</p>\n</div>";
        let doc = parse_markdown(input);

        let has_html_block = doc.elements.iter().any(|el| matches!(el, Element::Html(_)));
        assert!(has_html_block, "Should have HTML block element");
    }
}
