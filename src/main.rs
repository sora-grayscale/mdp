use clap::Parser;
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{self, Command, Stdio};
use tokio::sync::broadcast;

use mdp::files::FileTree;
use mdp::parser::parse_markdown;
use mdp::renderer::terminal::TerminalRenderer;
use mdp::server::{find_available_port, start_server};
use mdp::watcher::watch_file;

#[derive(Parser, Debug)]
#[command(name = "mdp")]
#[command(
    author,
    version,
    about = "A rich Markdown previewer for the terminal and browser"
)]
struct Args {
    /// Markdown file or directory to preview
    #[arg(required = true)]
    path: PathBuf,

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
    if !args.path.exists() {
        eprintln!("Error: Path not found: {}", args.path.display());
        process::exit(1);
    }

    // Build file tree (works for both file and directory)
    let file_tree = if args.path.is_dir() {
        match FileTree::from_directory(&args.path) {
            Ok(tree) => {
                if tree.files.is_empty() {
                    eprintln!(
                        "Error: No markdown files found in '{}'",
                        args.path.display()
                    );
                    process::exit(1);
                }
                tree
            }
            Err(e) => {
                eprintln!("Error: Failed to scan directory: {}", e);
                process::exit(1);
            }
        }
    } else {
        // Single file mode
        // Warn if file is not .md
        if let Some(ext) = args.path.extension() {
            if ext != "md" && ext != "markdown" {
                eprintln!(
                    "Warning: '{}' is not a markdown file (.md)",
                    args.path.display()
                );
                eprintln!("         Proceeding anyway...\n");
            }
        } else {
            eprintln!(
                "Warning: '{}' has no extension, treating as markdown\n",
                args.path.display()
            );
        }

        match FileTree::from_file(&args.path) {
            Ok(tree) => tree,
            Err(e) => {
                eprintln!("Error: Failed to read file: {}", e);
                process::exit(1);
            }
        }
    };

    // Get title from directory name or filename
    let title = if args.path.is_dir() {
        args.path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Markdown Preview")
            .to_string()
    } else {
        args.path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Markdown Preview")
            .to_string()
    };

    // Render based on mode
    if args.browser {
        // Browser mode (with optional watch)
        let port = find_available_port(args.port);
        let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
        if let Err(e) = rt.block_on(start_server(file_tree, &title, port, args.watch, args.toc)) {
            eprintln!("Error: Server failed: {}", e);
            process::exit(1);
        }
    } else if args.watch {
        // Terminal watch mode (single file only for now)
        if let Some(file) = file_tree.default_file() {
            run_terminal_watch_mode(&file.absolute_path, &args.theme, args.toc);
        }
    } else {
        // Normal terminal mode
        if file_tree.is_single_file() {
            if let Some(file) = file_tree.default_file() {
                run_terminal_mode(&file.absolute_path, &args.theme, args.no_pager, args.toc);
            }
        } else {
            // Directory mode in terminal - list files
            println!(
                "Found {} markdown files in '{}':\n",
                file_tree.files.len(),
                args.path.display()
            );
            for (i, file) in file_tree.files.iter().enumerate() {
                println!("  {}. {}", i + 1, file.relative_path.display());
            }
            println!("\nUse -b flag for browser mode with navigation sidebar.");
        }
    }
}

fn run_terminal_mode(file_path: &PathBuf, theme: &str, no_pager: bool, show_toc: bool) {
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
        if let Err(e) = renderer.render(&document, show_toc) {
            eprintln!("Error: Failed to render: {}", e);
            process::exit(1);
        }
    } else if let Err(e) = render_with_pager(&renderer, &document, show_toc) {
        eprintln!("Error: Failed to render: {}", e);
        process::exit(1);
    }
}

fn run_terminal_watch_mode(file_path: &PathBuf, theme: &str, show_toc: bool) {
    use crossterm::{
        ExecutableCommand, cursor,
        terminal::{self, ClearType},
    };

    let (tx, mut rx) = broadcast::channel::<()>(16);

    // Initial render
    render_terminal_content(file_path, theme, show_toc);

    // Start file watcher in a separate thread
    let watch_path = file_path.clone();
    std::thread::spawn(move || {
        if let Err(e) = watch_file(&watch_path, tx) {
            eprintln!("Watcher error: {}", e);
        }
    });

    println!("\n--- Watching for changes (Press Ctrl+C to exit) ---\n");

    // Wait for changes and re-render
    while rx.blocking_recv().is_ok() {
        // Clear screen and re-render
        let mut stdout = io::stdout();
        let _ = stdout.execute(terminal::Clear(ClearType::All));
        let _ = stdout.execute(cursor::MoveTo(0, 0));

        render_terminal_content(file_path, theme, show_toc);
        println!("\n--- Watching for changes (Press Ctrl+C to exit) ---\n");
    }
}

fn render_terminal_content(file_path: &PathBuf, theme: &str, show_toc: bool) {
    let content = match std::fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error: Failed to read file: {}", e);
            return;
        }
    };

    let document = parse_markdown(&content);
    let renderer = TerminalRenderer::new(theme);

    if let Err(e) = renderer.render(&document, show_toc) {
        eprintln!("Error: Failed to render: {}", e);
    }
}

fn render_with_pager(
    renderer: &TerminalRenderer,
    document: &mdp::parser::Document,
    show_toc: bool,
) -> io::Result<()> {
    // Render to buffer first
    let mut buffer = Vec::new();
    renderer.render_to_writer(&mut buffer, document, show_toc)?;

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
