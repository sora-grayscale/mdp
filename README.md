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

### Browser Mode
- GitHub-style rendering with CSS
- Live reload on file changes
- Syntax highlighting (powered by highlight.js)

### Planned Features
- Directory mode with file navigation
- KaTeX math rendering
- Mermaid diagram support
- Image display (iTerm2/Kitty protocol)
- Table of contents generation

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
# Basic usage (terminal)
mdp README.md

# Browser mode
mdp -b README.md

# Browser mode with live reload
mdp -bw README.md

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
| `-b, --browser` | Open in browser with GitHub-style rendering |
| `-w, --watch` | Watch for file changes and auto-reload |
| `-p, --port <PORT>` | Port for browser mode (default: 3000) |
| `--theme <THEME>` | Theme: dark or light (default: dark) |
| `--no-pager` | Disable pager, output directly to stdout |
| `--toc` | Show table of contents (not yet implemented) |

## Requirements

- Rust 1.70+
- A terminal with 24-bit color support (recommended)
- `less` or another pager (optional)

## Similar Projects

- [glow](https://github.com/charmbracelet/glow) - Render markdown on the CLI (Go)
- [mdcat](https://github.com/swsnr/mdcat) - cat for markdown (Rust)
- [grip](https://github.com/joeyespo/grip) - GitHub Readme Instant Preview (Python)
- [bat](https://github.com/sharkdp/bat) - A cat clone with syntax highlighting (Rust)
- [rich-cli](https://github.com/Textualize/rich-cli) - Fancy output in the terminal (Python)

## License

MIT

