use clap::Parser;
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{self, Command, Stdio};

use mdp::parser::parse_markdown;
use mdp::renderer::terminal::TerminalRenderer;

#[derive(Parser, Debug)]
#[command(name = "mdp")]
#[command(author, version, about = "A rich Markdown previewer for the terminal and browser")]
struct Args {
    /// Markdown file to preview
    #[arg(required = true)]
    file: PathBuf,

    /// Watch for file changes and re-render
    #[arg(short, long)]
    watch: bool,

    /// Open in browser instead of terminal
    #[arg(short, long)]
    browser: bool,

    /// Show table of contents
    #[arg(long)]
    toc: bool,

    /// Theme (dark or light)
    #[arg(long, default_value = "dark")]
    theme: String,

    /// Disable pager (output directly to stdout)
    #[arg(long)]
    no_pager: bool,
}

fn main() {
    let args = Args::parse();

    // Check if file exists
    if !args.file.exists() {
        eprintln!("Error: File not found: {}", args.file.display());
        process::exit(1);
    }

    // Read file content
    let content = match std::fs::read_to_string(&args.file) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error: Failed to read file: {}", e);
            process::exit(1);
        }
    };

    // Parse markdown
    let document = parse_markdown(&content);

    // Render based on mode
    if args.browser {
        // TODO: Phase 3 - Browser rendering
        eprintln!("Browser mode not yet implemented. Use terminal mode for now.");
        process::exit(1);
    } else {
        // Terminal rendering
        let renderer = TerminalRenderer::new(&args.theme);

        if args.no_pager || !atty::is(atty::Stream::Stdout) {
            // Direct output to stdout
            if let Err(e) = renderer.render(&document) {
                eprintln!("Error: Failed to render: {}", e);
                process::exit(1);
            }
        } else {
            // Use pager
            if let Err(e) = render_with_pager(&renderer, &document) {
                eprintln!("Error: Failed to render: {}", e);
                process::exit(1);
            }
        }
    }

    // TODO: Phase 4 - Watch mode
    if args.watch {
        eprintln!("Watch mode not yet implemented.");
    }
}

fn render_with_pager(
    renderer: &TerminalRenderer,
    document: &mdp::parser::Document,
) -> io::Result<()> {
    // Render to buffer first
    let mut buffer = Vec::new();
    renderer.render_to_writer(&mut buffer, document)?;

    // Get pager from environment or default to less
    let pager = env::var("PAGER").unwrap_or_else(|_| "less".to_string());
    let pager_args: Vec<&str> = if pager.contains("less") {
        vec!["-R", "-F", "-X"] // -R: raw control chars, -F: quit if one screen, -X: no init
    } else {
        vec![]
    };

    // Try to spawn pager
    match Command::new(&pager)
        .args(&pager_args)
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&buffer)?;
            }
            child.wait()?;
        }
        Err(_) => {
            // Fallback to direct output if pager fails
            io::stdout().write_all(&buffer)?;
        }
    }

    Ok(())
}
