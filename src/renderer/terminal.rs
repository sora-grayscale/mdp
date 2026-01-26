use crossterm::execute;
use crossterm::style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor};
use std::io::{self, Write};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;
use unicode_width::UnicodeWidthStr;

use crate::parser::{
    Alignment, Document, Element, InlineElement, ListItem, TocEntry, generate_toc,
};

pub struct TerminalRenderer {
    theme: String,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    term_width: usize,
}

impl TerminalRenderer {
    pub fn new(theme: &str) -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let term_width = crossterm::terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(80);

        Self {
            theme: theme.to_string(),
            syntax_set,
            theme_set,
            term_width,
        }
    }

    pub fn render(&self, document: &Document, show_toc: bool) -> io::Result<()> {
        self.render_to_writer(&mut io::stdout(), document, show_toc)
    }

    pub fn render_to_writer<W: Write>(
        &self,
        out: &mut W,
        document: &Document,
        show_toc: bool,
    ) -> io::Result<()> {
        // Render TOC if requested
        if show_toc {
            let toc = generate_toc(document);
            if !toc.is_empty() {
                self.render_toc(out, &toc)?;
            }
        }

        // Separate footnote definitions from other elements
        let mut footnotes = Vec::new();

        for element in &document.elements {
            if let Element::FootnoteDefinition { .. } = element {
                footnotes.push(element);
            } else {
                self.render_element(out, element, 0)?;
            }
        }

        // Render footnotes at the end with a separator
        if !footnotes.is_empty() {
            execute!(out, SetForegroundColor(Color::DarkGrey))?;
            writeln!(out, "{}", "─".repeat(self.term_width.min(40)))?;
            execute!(out, ResetColor)?;

            for footnote in footnotes {
                self.render_element(out, footnote, 0)?;
            }
        }

        Ok(())
    }

    fn render_toc<W: Write>(&self, out: &mut W, toc: &[TocEntry]) -> io::Result<()> {
        // TOC header
        writeln!(out)?;
        execute!(
            out,
            SetForegroundColor(Color::Cyan),
            SetAttribute(Attribute::Bold)
        )?;
        writeln!(out, "📑 Table of Contents")?;
        execute!(out, ResetColor, SetAttribute(Attribute::Reset))?;
        execute!(out, SetForegroundColor(Color::DarkGrey))?;
        writeln!(out, "{}", "─".repeat(self.term_width.min(30)))?;
        execute!(out, ResetColor)?;

        // Find minimum level for proper indentation
        let min_level = toc.iter().map(|e| e.level).min().unwrap_or(1);

        for entry in toc {
            let indent = "  ".repeat((entry.level - min_level) as usize);
            let bullet = match entry.level {
                1 => "●",
                2 => "○",
                3 => "◆",
                _ => "◇",
            };

            execute!(out, SetForegroundColor(Color::Cyan))?;
            write!(out, "{}{} ", indent, bullet)?;
            execute!(out, ResetColor)?;
            writeln!(out, "{}", entry.text)?;
        }

        writeln!(out)?;
        execute!(out, SetForegroundColor(Color::DarkGrey))?;
        writeln!(out, "{}", "━".repeat(self.term_width.min(50)))?;
        execute!(out, ResetColor)?;
        writeln!(out)?;

        Ok(())
    }

    fn render_element<W: Write>(
        &self,
        out: &mut W,
        element: &Element,
        indent: usize,
    ) -> io::Result<()> {
        match element {
            Element::Heading { level, content } => {
                self.render_heading(out, *level, content)?;
            }
            Element::Paragraph { content } => {
                self.render_paragraph(out, content, indent)?;
            }
            Element::CodeBlock { language, content } => {
                self.render_code_block(out, language.as_deref(), content)?;
            }
            Element::List {
                ordered,
                start,
                items,
            } => {
                self.render_list(out, *ordered, *start, items, indent)?;
            }
            Element::Table {
                headers,
                alignments,
                rows,
            } => {
                self.render_table(out, headers, alignments, rows)?;
            }
            Element::BlockQuote { content } => {
                self.render_blockquote(out, content)?;
            }
            Element::HorizontalRule => {
                self.render_horizontal_rule(out)?;
            }
            Element::Image { url, alt, .. } => {
                self.render_image(out, url, alt)?;
            }
            Element::FootnoteDefinition { label, content } => {
                self.render_footnote_definition(out, label, content)?;
            }
        }
        Ok(())
    }

    fn render_heading<W: Write>(&self, out: &mut W, level: u8, content: &str) -> io::Result<()> {
        let (color, prefix) = match level {
            1 => (Color::Magenta, "█ "),
            2 => (Color::Cyan, "▓ "),
            3 => (Color::Blue, "▒ "),
            4 => (Color::Green, "░ "),
            5 => (Color::Yellow, "• "),
            _ => (Color::White, "· "),
        };

        writeln!(out)?;
        execute!(
            out,
            SetForegroundColor(color),
            SetAttribute(Attribute::Bold)
        )?;
        write!(out, "{}", prefix)?;

        // Underline for h1 and h2
        if level <= 2 {
            execute!(out, SetAttribute(Attribute::Underlined))?;
        }

        writeln!(out, "{}", content)?;
        execute!(out, ResetColor, SetAttribute(Attribute::Reset))?;

        // Add decorative line for h1
        if level == 1 {
            execute!(out, SetForegroundColor(Color::DarkGrey))?;
            writeln!(
                out,
                "{}",
                "─".repeat(self.term_width.min(content.width() + 4))
            )?;
            execute!(out, ResetColor)?;
        }

        writeln!(out)?;
        Ok(())
    }

    fn render_paragraph<W: Write>(
        &self,
        out: &mut W,
        content: &[InlineElement],
        indent: usize,
    ) -> io::Result<()> {
        let indent_str = " ".repeat(indent);
        write!(out, "{}", indent_str)?;

        for inline in content {
            self.render_inline(out, inline)?;
        }

        writeln!(out)?;
        writeln!(out)?;
        Ok(())
    }

    #[allow(clippy::only_used_in_recursion)]
    fn render_inline<W: Write>(&self, out: &mut W, inline: &InlineElement) -> io::Result<()> {
        match inline {
            InlineElement::Text(text) => {
                write!(out, "{}", text)?;
            }
            InlineElement::Code(code) => {
                execute!(out, SetForegroundColor(Color::Yellow))?;
                write!(out, "`{}`", code)?;
                execute!(out, ResetColor)?;
            }
            InlineElement::Strong(content) => {
                execute!(out, SetAttribute(Attribute::Bold))?;
                for child in content {
                    self.render_inline(out, child)?;
                }
                execute!(out, SetAttribute(Attribute::Reset))?;
            }
            InlineElement::Emphasis(content) => {
                execute!(out, SetAttribute(Attribute::Italic))?;
                for child in content {
                    self.render_inline(out, child)?;
                }
                execute!(out, SetAttribute(Attribute::Reset))?;
            }
            InlineElement::Strikethrough(content) => {
                execute!(out, SetAttribute(Attribute::CrossedOut))?;
                for child in content {
                    self.render_inline(out, child)?;
                }
                execute!(out, SetAttribute(Attribute::Reset))?;
            }
            InlineElement::Link { url, content, .. } => {
                execute!(
                    out,
                    SetForegroundColor(Color::Blue),
                    SetAttribute(Attribute::Underlined)
                )?;
                for child in content {
                    self.render_inline(out, child)?;
                }
                execute!(out, ResetColor, SetAttribute(Attribute::Reset))?;
                execute!(out, SetForegroundColor(Color::DarkGrey))?;
                write!(out, " ({})", url)?;
                execute!(out, ResetColor)?;
            }
            InlineElement::FootnoteReference(label) => {
                execute!(out, SetForegroundColor(Color::Cyan))?;
                write!(out, "[^{}]", label)?;
                execute!(out, ResetColor)?;
            }
            InlineElement::SoftBreak | InlineElement::HardBreak => {
                writeln!(out)?;
            }
        }
        Ok(())
    }

    fn render_code_block<W: Write>(
        &self,
        out: &mut W,
        language: Option<&str>,
        content: &str,
    ) -> io::Result<()> {
        // Special handling for mermaid diagrams
        if language == Some("mermaid") {
            return self.render_mermaid_placeholder(out, content);
        }

        let syntax_theme = if self.theme == "light" {
            "base16-ocean.light"
        } else {
            "base16-ocean.dark"
        };

        // Get theme with fallback to first available theme
        let theme = self
            .theme_set
            .themes
            .get(syntax_theme)
            .or_else(|| self.theme_set.themes.values().next())
            .expect("No themes available in ThemeSet");

        // Find syntax for the language
        let syntax = language
            .and_then(|lang| self.syntax_set.find_syntax_by_token(lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, theme);

        // Draw top border
        execute!(out, SetForegroundColor(Color::DarkGrey))?;
        writeln!(out, "┌{}┐", "─".repeat(self.term_width.saturating_sub(2)))?;

        // Language label
        if let Some(lang) = language {
            execute!(out, SetForegroundColor(Color::Cyan))?;
            writeln!(out, "│ {}", lang)?;
            execute!(out, SetForegroundColor(Color::DarkGrey))?;
            writeln!(out, "├{}┤", "─".repeat(self.term_width.saturating_sub(2)))?;
        }

        execute!(out, ResetColor)?;

        // Render code with syntax highlighting
        for line in content.lines() {
            execute!(out, SetForegroundColor(Color::DarkGrey))?;
            write!(out, "│ ")?;
            execute!(out, ResetColor)?;

            let ranges: Vec<(Style, &str)> = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();
            let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
            write!(out, "{}", escaped)?;
            write!(out, "\x1b[0m")?; // Reset
            writeln!(out)?;
        }

        // Draw bottom border
        execute!(out, SetForegroundColor(Color::DarkGrey))?;
        writeln!(out, "└{}┘", "─".repeat(self.term_width.saturating_sub(2)))?;
        execute!(out, ResetColor)?;
        writeln!(out)?;

        Ok(())
    }

    fn render_list<W: Write>(
        &self,
        out: &mut W,
        ordered: bool,
        start: Option<u64>,
        items: &[ListItem],
        indent: usize,
    ) -> io::Result<()> {
        let indent_str = " ".repeat(indent);
        let mut number = start.unwrap_or(1);

        for item in items {
            let bullet = if ordered {
                let b = format!("{}. ", number);
                number += 1;
                b
            } else {
                match indent / 2 {
                    0 => "• ".to_string(),
                    1 => "◦ ".to_string(),
                    _ => "▪ ".to_string(),
                }
            };

            execute!(out, SetForegroundColor(Color::Cyan))?;
            write!(out, "{}{}", indent_str, bullet)?;
            execute!(out, ResetColor)?;

            for inline in &item.content {
                self.render_inline(out, inline)?;
            }
            writeln!(out)?;

            // Render nested list
            if let Some(ref sub_list) = item.sub_list {
                self.render_element(out, sub_list, indent + 2)?;
            }
        }

        if indent == 0 {
            writeln!(out)?;
        }

        Ok(())
    }

    fn render_table<W: Write>(
        &self,
        out: &mut W,
        headers: &[String],
        alignments: &[Alignment],
        rows: &[Vec<String>],
    ) -> io::Result<()> {
        // Determine number of columns
        let num_cols = headers
            .len()
            .max(rows.first().map(|r| r.len()).unwrap_or(0));
        if num_cols == 0 {
            return Ok(());
        }

        // Calculate column widths
        let mut col_widths: Vec<usize> = vec![0; num_cols];
        for (i, header) in headers.iter().enumerate() {
            if i < col_widths.len() {
                col_widths[i] = col_widths[i].max(header.width());
            }
        }
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cell.width());
                }
            }
        }

        // Add padding and ensure minimum width
        let col_widths: Vec<usize> = col_widths.iter().map(|w| (*w).max(3) + 2).collect();

        // Draw top border
        execute!(out, SetForegroundColor(Color::DarkGrey))?;
        write!(out, "┌")?;
        for (i, width) in col_widths.iter().enumerate() {
            write!(out, "{}", "─".repeat(*width))?;
            if i < col_widths.len() - 1 {
                write!(out, "┬")?;
            }
        }
        writeln!(out, "┐")?;

        // Draw header only if headers exist
        if !headers.is_empty() {
            execute!(out, SetForegroundColor(Color::DarkGrey))?;
            write!(out, "│")?;
            for (i, header) in headers.iter().enumerate() {
                let width = col_widths.get(i).copied().unwrap_or(10);
                let align = alignments.get(i).copied().unwrap_or(Alignment::Left);
                execute!(
                    out,
                    SetForegroundColor(Color::Cyan),
                    SetAttribute(Attribute::Bold)
                )?;
                write!(out, "{}", self.align_text(header, width, align))?;
                execute!(out, ResetColor, SetAttribute(Attribute::Reset))?;
                execute!(out, SetForegroundColor(Color::DarkGrey))?;
                write!(out, "│")?;
            }
            writeln!(out)?;

            // Draw header separator
            write!(out, "├")?;
            for (i, width) in col_widths.iter().enumerate() {
                write!(out, "{}", "─".repeat(*width))?;
                if i < col_widths.len() - 1 {
                    write!(out, "┼")?;
                }
            }
            writeln!(out, "┤")?;
        }

        // Draw rows
        for row in rows {
            write!(out, "│")?;
            for (i, cell) in row.iter().enumerate() {
                let width = col_widths.get(i).copied().unwrap_or(10);
                let align = alignments.get(i).copied().unwrap_or(Alignment::Left);
                execute!(out, ResetColor)?;
                write!(out, "{}", self.align_text(cell, width, align))?;
                execute!(out, SetForegroundColor(Color::DarkGrey))?;
                write!(out, "│")?;
            }
            writeln!(out)?;
        }

        // Draw bottom border
        write!(out, "└")?;
        for (i, width) in col_widths.iter().enumerate() {
            write!(out, "{}", "─".repeat(*width))?;
            if i < col_widths.len() - 1 {
                write!(out, "┴")?;
            }
        }
        writeln!(out, "┘")?;
        execute!(out, ResetColor)?;
        writeln!(out)?;

        Ok(())
    }

    fn align_text(&self, text: &str, width: usize, alignment: Alignment) -> String {
        let text_width = text.width();
        let padding = width.saturating_sub(text_width);

        match alignment {
            Alignment::Left | Alignment::None => {
                format!(" {}{}", text, " ".repeat(padding.saturating_sub(1)))
            }
            Alignment::Right => {
                format!("{}{} ", " ".repeat(padding.saturating_sub(1)), text)
            }
            Alignment::Center => {
                let left_pad = padding / 2;
                let right_pad = padding - left_pad;
                format!("{}{}{}", " ".repeat(left_pad), text, " ".repeat(right_pad))
            }
        }
    }

    fn render_blockquote<W: Write>(&self, out: &mut W, content: &[Element]) -> io::Result<()> {
        for element in content {
            match element {
                Element::Paragraph { content } => {
                    // First line
                    execute!(out, SetForegroundColor(Color::DarkGrey))?;
                    write!(out, "  ▌ ")?;
                    execute!(
                        out,
                        SetForegroundColor(Color::White),
                        SetAttribute(Attribute::Italic)
                    )?;

                    for inline in content {
                        match inline {
                            InlineElement::SoftBreak | InlineElement::HardBreak => {
                                writeln!(out)?;
                                execute!(out, ResetColor, SetAttribute(Attribute::Reset))?;
                                execute!(out, SetForegroundColor(Color::DarkGrey))?;
                                write!(out, "  ▌ ")?;
                                execute!(
                                    out,
                                    SetForegroundColor(Color::White),
                                    SetAttribute(Attribute::Italic)
                                )?;
                            }
                            _ => {
                                self.render_inline(out, inline)?;
                            }
                        }
                    }
                    writeln!(out)?;
                    execute!(out, ResetColor, SetAttribute(Attribute::Reset))?;
                }
                _ => {
                    execute!(out, SetForegroundColor(Color::DarkGrey))?;
                    write!(out, "  ▌ ")?;
                    execute!(out, ResetColor)?;
                    self.render_element(out, element, 4)?;
                }
            }
        }
        writeln!(out)?;
        Ok(())
    }

    fn render_horizontal_rule<W: Write>(&self, out: &mut W) -> io::Result<()> {
        execute!(out, SetForegroundColor(Color::DarkGrey))?;
        writeln!(out)?;
        writeln!(out, "{}", "━".repeat(self.term_width))?;
        writeln!(out)?;
        execute!(out, ResetColor)?;
        Ok(())
    }

    fn render_image<W: Write>(&self, out: &mut W, url: &str, alt: &str) -> io::Result<()> {
        // For now, just display image info
        // TODO: Phase 5 - iTerm2/Kitty image protocol support
        execute!(out, SetForegroundColor(Color::Magenta))?;
        write!(out, "🖼  ")?;
        execute!(
            out,
            SetForegroundColor(Color::Blue),
            SetAttribute(Attribute::Underlined)
        )?;
        write!(out, "{}", if alt.is_empty() { "Image" } else { alt })?;
        execute!(out, ResetColor, SetAttribute(Attribute::Reset))?;
        execute!(out, SetForegroundColor(Color::DarkGrey))?;
        writeln!(out, " ({})", url)?;
        execute!(out, ResetColor)?;
        writeln!(out)?;
        Ok(())
    }

    fn render_footnote_definition<W: Write>(
        &self,
        out: &mut W,
        label: &str,
        content: &[Element],
    ) -> io::Result<()> {
        // Render footnote label
        execute!(out, SetForegroundColor(Color::Cyan))?;
        write!(out, "[^{}]: ", label)?;
        execute!(out, ResetColor)?;

        // Render footnote content inline if it's a single paragraph
        if content.len() == 1 {
            if let Element::Paragraph {
                content: inline_content,
            } = &content[0]
            {
                for inline in inline_content {
                    self.render_inline(out, inline)?;
                }
                writeln!(out)?;
                writeln!(out)?;
                return Ok(());
            }
        }

        // Otherwise render each element with indent
        writeln!(out)?;
        for element in content {
            self.render_element(out, element, 4)?;
        }
        Ok(())
    }

    fn render_mermaid_placeholder<W: Write>(&self, out: &mut W, content: &str) -> io::Result<()> {
        let box_width = self.term_width.saturating_sub(2);

        // Draw mermaid header
        execute!(out, SetForegroundColor(Color::Magenta))?;
        writeln!(out, "┌{}┐", "─".repeat(box_width))?;
        writeln!(
            out,
            "│ 🧜 Mermaid Diagram {:>width$}│",
            "",
            width = box_width - 21
        )?;
        execute!(out, SetForegroundColor(Color::DarkGrey))?;
        writeln!(out, "├{}┤", "─".repeat(box_width))?;

        // Draw mermaid code
        execute!(out, ResetColor)?;
        for line in content.lines() {
            execute!(out, SetForegroundColor(Color::DarkGrey))?;
            write!(out, "│ ")?;
            execute!(out, SetForegroundColor(Color::Cyan))?;
            let line_display = if line.width() > box_width - 3 {
                format!("{}...", &line[..box_width.saturating_sub(6)])
            } else {
                line.to_string()
            };
            write!(out, "{:width$}", line_display, width = box_width - 2)?;
            execute!(out, SetForegroundColor(Color::DarkGrey))?;
            writeln!(out, "│")?;
        }

        // Draw footer with hint
        writeln!(out, "├{}┤", "─".repeat(box_width))?;
        execute!(out, SetForegroundColor(Color::DarkGrey))?;
        let hint = "(View rendered diagram: mdp -b)";
        writeln!(out, "│{:^width$}│", hint, width = box_width)?;
        writeln!(out, "└{}┘", "─".repeat(box_width))?;
        execute!(out, ResetColor)?;
        writeln!(out)?;

        Ok(())
    }
}
