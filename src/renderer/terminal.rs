use crossterm::style::{Attribute, Color, SetAttribute, SetForegroundColor, ResetColor};
use crossterm::execute;
use std::io::{self, Write};
use syntect::easy::HighlightLines;
use syntect::highlighting::{ThemeSet, Style};
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;
use unicode_width::UnicodeWidthStr;

use crate::parser::{Document, Element, InlineElement, Alignment, ListItem};

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

    pub fn render(&self, document: &Document) -> io::Result<()> {
        self.render_to_writer(&mut io::stdout(), document)
    }

    pub fn render_to_writer<W: Write>(&self, out: &mut W, document: &Document) -> io::Result<()> {
        for element in &document.elements {
            self.render_element(out, element, 0)?;
        }

        Ok(())
    }

    fn render_element<W: Write>(&self, out: &mut W, element: &Element, indent: usize) -> io::Result<()> {
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
            Element::List { ordered, start, items } => {
                self.render_list(out, *ordered, *start, items, indent)?;
            }
            Element::Table { headers, alignments, rows } => {
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
        execute!(out, SetForegroundColor(color), SetAttribute(Attribute::Bold))?;
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
            writeln!(out, "{}", "─".repeat(self.term_width.min(content.width() + 4)))?;
            execute!(out, ResetColor)?;
        }

        writeln!(out)?;
        Ok(())
    }

    fn render_paragraph<W: Write>(&self, out: &mut W, content: &[InlineElement], indent: usize) -> io::Result<()> {
        let indent_str = " ".repeat(indent);
        write!(out, "{}", indent_str)?;

        for inline in content {
            self.render_inline(out, inline)?;
        }

        writeln!(out)?;
        writeln!(out)?;
        Ok(())
    }

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
            InlineElement::Strong(text) => {
                execute!(out, SetAttribute(Attribute::Bold))?;
                write!(out, "{}", text)?;
                execute!(out, SetAttribute(Attribute::Reset))?;
            }
            InlineElement::Emphasis(text) => {
                execute!(out, SetAttribute(Attribute::Italic))?;
                write!(out, "{}", text)?;
                execute!(out, SetAttribute(Attribute::Reset))?;
            }
            InlineElement::Strikethrough(text) => {
                execute!(out, SetAttribute(Attribute::CrossedOut))?;
                write!(out, "{}", text)?;
                execute!(out, SetAttribute(Attribute::Reset))?;
            }
            InlineElement::Link { url, text, .. } => {
                execute!(out, SetForegroundColor(Color::Blue), SetAttribute(Attribute::Underlined))?;
                write!(out, "{}", text)?;
                execute!(out, ResetColor, SetAttribute(Attribute::Reset))?;
                execute!(out, SetForegroundColor(Color::DarkGrey))?;
                write!(out, " ({})", url)?;
                execute!(out, ResetColor)?;
            }
            InlineElement::SoftBreak | InlineElement::HardBreak => {
                writeln!(out)?;
            }
        }
        Ok(())
    }

    fn render_code_block<W: Write>(&self, out: &mut W, language: Option<&str>, content: &str) -> io::Result<()> {
        let syntax_theme = if self.theme == "light" {
            "base16-ocean.light"
        } else {
            "base16-ocean.dark"
        };

        let theme = &self.theme_set.themes[syntax_theme];

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

    fn render_list<W: Write>(&self, out: &mut W, ordered: bool, start: Option<u64>, items: &[ListItem], indent: usize) -> io::Result<()> {
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

    fn render_table<W: Write>(&self, out: &mut W, headers: &[String], alignments: &[Alignment], rows: &[Vec<String>]) -> io::Result<()> {
        // Determine number of columns
        let num_cols = headers.len().max(rows.first().map(|r| r.len()).unwrap_or(0));
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

        // Draw header
        execute!(out, SetForegroundColor(Color::DarkGrey))?;
        write!(out, "│")?;
        for (i, header) in headers.iter().enumerate() {
            let width = col_widths.get(i).copied().unwrap_or(10);
            let align = alignments.get(i).copied().unwrap_or(Alignment::Left);
            execute!(out, SetForegroundColor(Color::Cyan), SetAttribute(Attribute::Bold))?;
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
                    execute!(out, SetForegroundColor(Color::White), SetAttribute(Attribute::Italic))?;

                    for inline in content {
                        match inline {
                            InlineElement::SoftBreak | InlineElement::HardBreak => {
                                writeln!(out)?;
                                execute!(out, ResetColor, SetAttribute(Attribute::Reset))?;
                                execute!(out, SetForegroundColor(Color::DarkGrey))?;
                                write!(out, "  ▌ ")?;
                                execute!(out, SetForegroundColor(Color::White), SetAttribute(Attribute::Italic))?;
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
        execute!(out, SetForegroundColor(Color::Blue), SetAttribute(Attribute::Underlined))?;
        write!(out, "{}", if alt.is_empty() { "Image" } else { alt })?;
        execute!(out, ResetColor, SetAttribute(Attribute::Reset))?;
        execute!(out, SetForegroundColor(Color::DarkGrey))?;
        writeln!(out, " ({})", url)?;
        execute!(out, ResetColor)?;
        writeln!(out)?;
        Ok(())
    }
}
