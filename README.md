# mdp - Markdown Preview

A rich Markdown previewer for the terminal and browser, written in Rust.

## Features

### Terminal Mode
- Syntax highlighting for code blocks (powered by syntect)
- Beautiful Unicode tables with box-drawing characters
- Nested lists (bullet, numbered)
- Blockquotes with visual indicators
- Links with URL display
- Horizontal rules
- Bold, italic, strikethrough text
- Inline code highlighting
- Automatic paging with less

### Planned Features
- Browser mode with live reload
- File watching for auto-refresh
- KaTeX math rendering
- Mermaid diagram support
- Image display (iTerm2/Kitty protocol)
- Table of contents generation
- Theme customization (dark/light)

## Installation

### From Source

```bash
git clone https://github.com/sora-grayscale/mdp.git
cd mdp
cargo build --release
sudo cp target/release/mdp /usr/local/bin/
```

## Usage

```bash
# Basic usage
mdp README.md

# Disable pager (output directly)
mdp --no-pager README.md

# Specify theme
mdp --theme light README.md

# Show help
mdp --help
```

### Options

| Option | Description |
|--------|-------------|
| `-w, --watch` | Watch for file changes (not yet implemented) |
| `-b, --browser` | Open in browser (not yet implemented) |
| `--toc` | Show table of contents (not yet implemented) |
| `--theme <THEME>` | Theme: dark or light (default: dark) |
| `--no-pager` | Disable pager, output directly to stdout |

## Requirements

- Rust 1.70+
- A terminal with 24-bit color support (recommended)
- `less` or another pager (optional)

## License

MIT
