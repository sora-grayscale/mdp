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
- Footnotes support
- Table of contents generation (`--toc`)
- Automatic paging with less
- Watch mode with live reload

### Browser Mode
- GitHub-style rendering with CSS
- Dark/Light theme toggle with system preference detection
- Live reload on file changes
- Syntax highlighting (powered by highlight.js)
- Directory mode with sidebar navigation
- Collapsible folder tree in sidebar
- External links open in new tab
- Footnotes support
- Table of contents generation (`--toc`)
- Auto-shutdown when browser tab closes

### Planned Features
- KaTeX math rendering
- Mermaid diagram support
- Image display (iTerm2/Kitty protocol)

## Installation

### From Source

```bash
git clone https://github.com/sora-grayscale/mdp.git
cd mdp
cargo build --release
sudo cp target/release/mdp /usr/local/bin/
```

### From Releases

Download the pre-built binary for your platform from the [Releases](https://github.com/sora-grayscale/mdp/releases) page.

## Usage

```bash
# Basic usage (terminal)
mdp README.md

# Browser mode
mdp -b README.md

# Browser mode with live reload
mdp -bw README.md

# Directory mode (browse multiple files)
mdp -b ./docs

# Disable pager (output directly)
mdp --no-pager README.md

# Show table of contents
mdp --toc README.md

# Browser mode with TOC
mdp -b --toc README.md

# Specify theme (terminal mode)
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
| `--toc` | Show table of contents at document top |

## Requirements

- Rust 1.85+ (edition 2024)
- A terminal with 24-bit color support (recommended)
- `less` or another pager (optional)

## Development

```bash
# Run tests
cargo test

# Run with clippy
cargo clippy --all-targets --all-features -- -D warnings

# Format code
cargo fmt
```

## Similar Projects

- [glow](https://github.com/charmbracelet/glow) - Terminal markdown viewer with TUI (mdp is simpler, focuses on quick preview)
- [mdcat](https://github.com/swsnr/mdcat) - Terminal markdown renderer (mdp adds browser mode and live reload)
- [grip](https://github.com/joeyespo/grip) - GitHub preview via API (mdp works offline, no API needed)
- [bat](https://github.com/sharkdp/bat) - Syntax highlighter (mdp renders markdown structure, not just highlights)
- [rich-cli](https://github.com/Textualize/rich-cli) - Rich terminal output (mdp is markdown-focused with browser mode)

## License

[MIT](./LICENSE)
