use clap::Parser;
use std::env;
use std::io::{self, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use tokio::sync::broadcast;

use mdp::files::FileTree;
use mdp::parser::parse_markdown;
use mdp::renderer::terminal::TerminalRenderer;
use mdp::server::{browse_addr, find_available_port, get_local_ip, start_server};
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

    /// Show sidebar with related markdown files (for single file mode)
    #[arg(short, long)]
    sidebar: bool,

    /// Theme (dark or light)
    #[arg(long, default_value = "dark")]
    theme: String,

    /// Disable pager (output directly to stdout)
    #[arg(long)]
    no_pager: bool,

    /// Port for browser mode (default: 3000, auto-increments if busy)
    #[arg(short, long, default_value = "3000")]
    port: u16,

    /// Bind to specified address for LAN access (default: 0.0.0.0 if no value given)
    #[arg(long, default_missing_value = "0.0.0.0", num_args = 0..=1, require_equals = true)]
    host: Option<String>,

    /// Run browser server in background (returns control to terminal)
    #[arg(short = 'B', long)]
    background: bool,

    /// Internal flag: indicates this process is the daemonized child
    #[arg(long = "_daemon", hide = true)]
    daemon: bool,
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

        // Use context mode if sidebar option is enabled
        if args.sidebar {
            match FileTree::from_file_with_context(&args.path) {
                Ok(tree) => tree,
                Err(e) => {
                    eprintln!("Error: Failed to scan directory: {}", e);
                    process::exit(1);
                }
            }
        } else {
            match FileTree::from_file(&args.path) {
                Ok(tree) => tree,
                Err(e) => {
                    eprintln!("Error: Failed to read file: {}", e);
                    process::exit(1);
                }
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

    // Validate: --background requires --browser
    if args.background && !args.browser {
        eprintln!("Error: --background (-B) requires --browser (-b) mode");
        process::exit(1);
    }

    // Validate: --host requires --browser
    if args.host.is_some() && !args.browser {
        eprintln!("Warning: --host is ignored without --browser (-b) mode");
    }

    // Validate host address format
    if let Some(ref host) = args.host {
        if host.parse::<IpAddr>().is_err() {
            eprintln!("Error: Invalid host address: {}", host);
            process::exit(1);
        }
    }

    // Render based on mode
    if args.browser {
        let host = args.host.as_deref().unwrap_or("127.0.0.1");

        // Background mode: check for existing server first
        if args.background && !args.daemon {
            let pid_file = pid_file_path(args.port);
            if pid_file.exists() {
                if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
                    if let Ok(pid) = pid_str.trim().parse::<u32>() {
                        if is_process_alive(pid) {
                            let browse_host = browse_addr(host);
                            println!(
                                "Server already running at http://{}:{} (PID: {})",
                                browse_host, args.port, pid
                            );
                            let _ = open::that(format!("http://{}:{}", browse_host, args.port));
                            return;
                        }
                    }
                }
                // Stale PID file, remove it
                let _ = std::fs::remove_file(&pid_file);
            }

            let port = find_available_port(host, args.port);
            spawn_background_server(&args.path, host, port, args.watch, args.toc, args.sidebar);
            return;
        }

        let port = find_available_port(host, args.port);

        // Normal or daemon browser mode
        let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");

        // Write PID file if running as daemon
        let pid_file = if args.daemon {
            let path = pid_file_path(port);
            if let Err(e) = std::fs::write(&path, process::id().to_string()) {
                eprintln!("Warning: Failed to write PID file: {}", e);
            }
            Some(path)
        } else {
            None
        };

        let open_browser = !args.daemon;
        let result = rt.block_on(start_server(
            file_tree,
            &title,
            host,
            port,
            args.watch,
            args.toc,
            open_browser,
        ));

        // Clean up PID file
        if let Some(path) = pid_file {
            let _ = std::fs::remove_file(path);
        }

        if let Err(e) = result {
            eprintln!("Error: Server failed: {}", e);
            process::exit(1);
        }

        // Force exit to terminate blocking watcher threads
        // (spawn_blocking tasks in file watchers run infinite loops
        //  that can't be cancelled by tokio runtime shutdown)
        process::exit(0);
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
        event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
        terminal::{self, ClearType},
    };
    use std::time::Duration;

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

    println!("\n--- Watching for changes (Press q or Ctrl+C to exit) ---\n");

    // Enable raw mode for keyboard input with drop guard for panic safety
    struct RawModeGuard;
    impl Drop for RawModeGuard {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
    let _ = terminal::enable_raw_mode();
    let _raw_guard = RawModeGuard;

    loop {
        // Poll for keyboard events (non-blocking with 100ms timeout)
        if event::poll(Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(KeyEvent {
                code, modifiers, ..
            })) = event::read()
            {
                match (code, modifiers) {
                    // Exit on 'q' or 'Q'
                    (KeyCode::Char('q'), KeyModifiers::NONE)
                    | (KeyCode::Char('Q'), KeyModifiers::SHIFT) => {
                        break;
                    }
                    // Exit on Ctrl+C
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        break;
                    }
                    _ => {}
                }
            }
        }

        // Check for file changes (non-blocking)
        if let Ok(()) = rx.try_recv() {
            // Clear screen and re-render
            let mut stdout = io::stdout();
            let _ = stdout.execute(terminal::Clear(ClearType::All));
            let _ = stdout.execute(cursor::MoveTo(0, 0));

            render_terminal_content(file_path, theme, show_toc);
            println!("\n--- Watching for changes (Press q or Ctrl+C to exit) ---\n");
        }
    }

    // _raw_guard dropped here, restoring terminal state
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
                // If write fails, fall back to direct output
                if stdin.write_all(&buffer).is_err() {
                    drop(stdin);
                    let _ = child.kill();
                    io::stdout().write_all(&buffer)?;
                    return Ok(());
                }
            }
            // Wait for pager to finish
            match child.wait() {
                Ok(status) => {
                    // Exit codes: 0 = normal, 1 = quit early (less 'q'), others = error
                    // Only warn on actual errors (signal termination, etc.)
                    if !status.success() {
                        #[cfg(unix)]
                        if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status)
                        {
                            // Signal termination (e.g., SIGPIPE) - not an error for pagers
                            if signal != 13 {
                                // 13 = SIGPIPE, which is normal
                                eprintln!("Pager terminated by signal {}", signal);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to wait for pager: {}", e);
                }
            }
        }
        Err(_) => {
            // Fallback to direct output if pager fails to spawn
            io::stdout().write_all(&buffer)?;
        }
    }

    Ok(())
}

/// Get PID file path for a given port
fn pid_file_path(port: u16) -> PathBuf {
    std::env::temp_dir().join(format!("mdp-{}.pid", port))
}

/// Check if a process with the given PID is still running
fn is_process_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Wait for the server to be ready on the given port
fn wait_for_port(host: &str, port: u16, timeout_ms: u64) -> bool {
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    let connect_host = browse_addr(host);
    let addr = format!("{}:{}", connect_host, port);

    while start.elapsed() < timeout {
        if TcpStream::connect(&addr).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Spawn a detached background server process
fn spawn_background_server(path: &Path, host: &str, port: u16, watch: bool, toc: bool, sidebar: bool) {
    let exe = env::current_exe().unwrap_or_else(|e| {
        eprintln!("Error: Failed to get current executable: {}", e);
        process::exit(1);
    });

    let mut cmd_args = vec![
        path.to_string_lossy().to_string(),
        "-b".to_string(),
        "--_daemon".to_string(),
        "-p".to_string(),
        port.to_string(),
        format!("--host={}", host),
    ];
    if watch {
        cmd_args.push("-w".to_string());
    }
    if toc {
        cmd_args.push("--toc".to_string());
    }
    if sidebar {
        cmd_args.push("-s".to_string());
    }

    match Command::new(&exe)
        .args(&cmd_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => {
            let pid = child.id();
            let browse_host = browse_addr(host);
            println!("Server running at: (background, PID: {})", pid);
            println!("  Local:   http://{}:{}", browse_host, port);
            if host == "0.0.0.0" {
                if let Some(lan_ip) = get_local_ip() {
                    println!("  Network: http://{}:{}", lan_ip, port);
                }
            }
            println!("PID file: {}", pid_file_path(port).display());
            println!("Stop with: kill {}", pid);

            // Wait for server to be ready, then open browser
            if wait_for_port(host, port, 5000) {
                let _ = open::that(format!("http://{}:{}", browse_host, port));
            } else {
                eprintln!(
                    "Warning: Server did not start within 5s, open manually: http://{}:{}",
                    browse_host, port
                );
            }
        }
        Err(e) => {
            eprintln!("Error: Failed to start background server: {}", e);
            process::exit(1);
        }
    }
}
