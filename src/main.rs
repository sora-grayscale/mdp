use clap::Parser;
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{self, Command, Stdio};
use tokio::sync::broadcast;

use mdp::parser::parse_markdown;
use mdp::renderer::terminal::TerminalRenderer;
use mdp::server::{find_available_port, start_server};
use mdp::watcher::watch_file;

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

    /// Port for browser mode (default: auto-select)
    #[arg(short, long, default_value = "3000")]
    port: u16,
}

fn main() {
    let args = Args::parse();

    // Check if path exists
    if !args.file.exists() {
        eprintln!("Error: File not found: {}", args.file.display());
        process::exit(1);
    }

    // Check if it's a directory
    if args.file.is_dir() {
        eprintln!("Error: '{}' is a directory", args.file.display());
        eprintln!("Hint: Specify a markdown file, e.g., mdp README.md");
        eprintln!("      Directory mode coming soon! (Issue #6)");
        process::exit(1);
    }

    // Get absolute path
    let file_path = args.file.canonicalize().unwrap_or_else(|_| args.file.clone());

    // Warn if file is not .md
    if let Some(ext) = file_path.extension() {
        if ext != "md" && ext != "markdown" {
            eprintln!("Warning: '{}' is not a markdown file (.md)", args.file.display());
            eprintln!("         Proceeding anyway...\n");
        }
    } else {
        eprintln!("Warning: '{}' has no extension, treating as markdown\n", args.file.display());
    }

    // Get title from filename
    let title = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Markdown Preview")
        .to_string();

    // Render based on mode
    if args.browser {
        // Browser mode (with optional watch)
        let port = find_available_port(args.port);
        let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
        if let Err(e) = rt.block_on(start_server(file_path, &title, port, args.watch)) {
            eprintln!("Error: Server failed: {}", e);
            process::exit(1);
        }
    } else if args.watch {
        // Terminal watch mode
        run_terminal_watch_mode(&file_path, &args.theme, args.no_pager);
    } else {
        // Normal terminal mode
        run_terminal_mode(&file_path, &args.theme, args.no_pager);
    }
}

fn run_terminal_mode(file_path: &PathBuf, theme: &str, no_pager: bool) {
    let content = match std::fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error: Failed to read file: {}", e);
            process::exit(1);
        }
    };

    let document = parse_markdown(&content);
    let renderer = TerminalRenderer::new(theme);

    if no_pager || !atty::is(atty::Stream::Stdout) {
        if let Err(e) = renderer.render(&document) {
            eprintln!("Error: Failed to render: {}", e);
            process::exit(1);
        }
    } else {
        if let Err(e) = render_with_pager(&renderer, &document) {
            eprintln!("Error: Failed to render: {}", e);
            process::exit(1);
        }
    }
}

fn run_terminal_watch_mode(file_path: &PathBuf, theme: &str, _no_pager: bool) {
    use crossterm::{
        cursor,
        terminal::{self, ClearType},
        ExecutableCommand,
    };

    let (tx, mut rx) = broadcast::channel::<()>(16);

    // Initial render
    render_terminal_content(file_path, theme);

    // Start file watcher in a separate thread
    let watch_path = file_path.clone();
    std::thread::spawn(move || {
        if let Err(e) = watch_file(&watch_path, tx) {
            eprintln!("Watcher error: {}", e);
        }
    });

    println!("\n--- Watching for changes (Press Ctrl+C to exit) ---\n");

    // Wait for changes and re-render
    loop {
        match rx.blocking_recv() {
            Ok(_) => {
                // Clear screen and re-render
                let mut stdout = io::stdout();
                let _ = stdout.execute(terminal::Clear(ClearType::All));
                let _ = stdout.execute(cursor::MoveTo(0, 0));

                render_terminal_content(file_path, theme);
                println!("\n--- Watching for changes (Press Ctrl+C to exit) ---\n");
            }
            Err(_) => break,
        }
    }
}

fn render_terminal_content(file_path: &PathBuf, theme: &str) {
    let content = match std::fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error: Failed to read file: {}", e);
            return;
        }
    };

    let document = parse_markdown(&content);
    let renderer = TerminalRenderer::new(theme);

    if let Err(e) = renderer.render(&document) {
        eprintln!("Error: Failed to render: {}", e);
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
