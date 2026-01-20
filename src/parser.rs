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
}

#[derive(Debug, Clone)]
pub struct ListItem {
    pub content: Vec<InlineElement>,
    pub sub_list: Option<Box<Element>>,
}

#[derive(Debug, Clone)]
pub enum InlineElement {
    Text(String),
    Code(String),
    Strong(String),
    Emphasis(String),
    Strikethrough(String),
    Link {
        url: String,
        text: String,
        title: Option<String>,
    },
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
            let mut inline_elements = Vec::new();
            let mut index = start + 1;

            while index < events.len() {
                match &events[index] {
                    Event::End(TagEnd::Paragraph) => {
                        break;
                    }
                    Event::Text(text) => {
                        inline_elements.push(InlineElement::Text(text.to_string()));
                    }
                    Event::Code(code) => {
                        inline_elements.push(InlineElement::Code(code.to_string()));
                    }
                    Event::Start(Tag::Strong) => {
                        let mut text = String::new();
                        index += 1;
                        while index < events.len() {
                            match &events[index] {
                                Event::End(TagEnd::Strong) => break,
                                Event::Text(t) => text.push_str(t),
                                _ => {}
                            }
                            index += 1;
                        }
                        inline_elements.push(InlineElement::Strong(text));
                    }
                    Event::Start(Tag::Emphasis) => {
                        let mut text = String::new();
                        index += 1;
                        while index < events.len() {
                            match &events[index] {
                                Event::End(TagEnd::Emphasis) => break,
                                Event::Text(t) => text.push_str(t),
                                _ => {}
                            }
                            index += 1;
                        }
                        inline_elements.push(InlineElement::Emphasis(text));
                    }
                    Event::Start(Tag::Strikethrough) => {
                        let mut text = String::new();
                        index += 1;
                        while index < events.len() {
                            match &events[index] {
                                Event::End(TagEnd::Strikethrough) => break,
                                Event::Text(t) => text.push_str(t),
                                _ => {}
                            }
                            index += 1;
                        }
                        inline_elements.push(InlineElement::Strikethrough(text));
                    }
                    Event::Start(Tag::Link {
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
                        let mut text = String::new();
                        index += 1;
                        while index < events.len() {
                            match &events[index] {
                                Event::End(TagEnd::Link) => break,
                                Event::Text(t) => text.push_str(t),
                                _ => {}
                            }
                            index += 1;
                        }
                        inline_elements.push(InlineElement::Link { url, text, title });
                    }
                    Event::SoftBreak => {
                        inline_elements.push(InlineElement::SoftBreak);
                    }
                    Event::HardBreak => {
                        inline_elements.push(InlineElement::HardBreak);
                    }
                    _ => {}
                }
                index += 1;
            }

            (
                Some(Element::Paragraph {
                    content: inline_elements,
                }),
                index + 1,
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
                        let mut item_content = Vec::new();
                        let mut sub_list = None;
                        index += 1;

                        while index < events.len() {
                            match &events[index] {
                                Event::End(TagEnd::Item) => {
                                    break;
                                }
                                Event::Text(text) => {
                                    item_content.push(InlineElement::Text(text.to_string()));
                                }
                                Event::Code(code) => {
                                    item_content.push(InlineElement::Code(code.to_string()));
                                }
                                Event::Start(Tag::List(_)) => {
                                    let (nested, new_index) = parse_element(events, index);
                                    if let Some(list) = nested {
                                        sub_list = Some(Box::new(list));
                                    }
                                    index = new_index - 1;
                                }
                                Event::Start(Tag::Strong) => {
                                    let mut text = String::new();
                                    index += 1;
                                    while index < events.len() {
                                        match &events[index] {
                                            Event::End(TagEnd::Strong) => break,
                                            Event::Text(t) => text.push_str(t),
                                            _ => {}
                                        }
                                        index += 1;
                                    }
                                    item_content.push(InlineElement::Strong(text));
                                }
                                Event::Start(Tag::Emphasis) => {
                                    let mut text = String::new();
                                    index += 1;
                                    while index < events.len() {
                                        match &events[index] {
                                            Event::End(TagEnd::Emphasis) => break,
                                            Event::Text(t) => text.push_str(t),
                                            _ => {}
                                        }
                                        index += 1;
                                    }
                                    item_content.push(InlineElement::Emphasis(text));
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
                                    let mut text = String::new();
                                    index += 1;
                                    while index < events.len() {
                                        match &events[index] {
                                            Event::End(TagEnd::Link) => break,
                                            Event::Text(t) => text.push_str(t),
                                            _ => {}
                                        }
                                        index += 1;
                                    }
                                    item_content.push(InlineElement::Link { url, text, title });
                                }
                                _ => {}
                            }
                            index += 1;
                        }

                        items.push(ListItem {
                            content: item_content,
                            sub_list,
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
            let mut _in_head = false;
            let mut current_row = Vec::new();
            let mut current_cell = String::new();

            while index < events.len() {
                match &events[index] {
                    Event::End(TagEnd::Table) => {
                        break;
                    }
                    Event::Start(Tag::TableHead) => {
                        _in_head = true;
                        current_row = Vec::new();
                    }
                    Event::End(TagEnd::TableHead) => {
                        // TableHead contains cells directly without TableRow in pulldown-cmark 0.10
                        headers = current_row.clone();
                        _in_head = false;
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

        _ => (None, start + 1),
    }
}
