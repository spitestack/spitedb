//! SpiteStack compiler CLI.
//!
//! Long Island Grit - raw, distorted, emotionally intense.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};
use tokio::process::{Child, Command};

use spite_compiler::{Compiler, CompilerConfig};

mod tui;
mod ui;

#[derive(Parser)]
#[command(name = "spitestack")]
#[command(about = "SpiteStack compiler - compiles domain logic to TypeScript")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new SpiteStack project
    Init {
        /// Project directory (created if doesn't exist)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Domain source directory (relative to project)
        #[arg(short, long, default_value = "src/domain")]
        domain: PathBuf,

        /// Source language
        #[arg(short, long, default_value = "typescript")]
        language: String,

        /// Port for the dev server
        #[arg(short, long, default_value_t = 3000)]
        port: u16,
    },

    /// Compile domain logic to a TypeScript project
    Compile {
        /// Domain source directory
        #[arg(short, long, default_value = "src/domain")]
        domain: PathBuf,

        /// Output directory for generated TypeScript project
        #[arg(short, long, default_value = ".spitestack")]
        output: PathBuf,

        /// Source language
        #[arg(short, long, default_value = "typescript")]
        language: String,

        /// Skip purity checks
        #[arg(long)]
        skip_purity_check: bool,

        /// Port for the generated server
        #[arg(short, long, default_value_t = 3000)]
        port: u16,
    },

    /// Check domain logic without generating code
    Check {
        /// Domain source directory
        #[arg(short, long, default_value = "src/domain")]
        domain: PathBuf,

        /// Source language
        #[arg(short, long, default_value = "typescript")]
        language: String,
    },

    /// Start dev server with hot reload
    Dev {
        /// Domain source directory
        #[arg(short, long, default_value = "src/domain")]
        domain: PathBuf,

        /// Output directory for generated TypeScript project
        #[arg(short, long, default_value = ".spitestack")]
        output: PathBuf,

        /// Source language
        #[arg(short, long, default_value = "typescript")]
        language: String,

        /// Port for the dev server
        #[arg(short, long, default_value_t = 3000)]
        port: u16,

        /// Skip purity checks
        #[arg(long)]
        skip_purity_check: bool,
    },

    /// Watch for changes and recompile (without running)
    Watch {
        /// Domain source directory
        #[arg(short, long, default_value = "src/domain")]
        domain: PathBuf,

        /// Output directory for generated TypeScript project
        #[arg(short, long, default_value = ".spitestack")]
        output: PathBuf,

        /// Source language
        #[arg(short, long, default_value = "typescript")]
        language: String,
    },

    /// Schema management commands for event evolution
    Schema {
        #[command(subcommand)]
        action: SchemaAction,
    },
}

/// Schema management subcommands.
#[derive(Subcommand)]
enum SchemaAction {
    /// Show current schema status and any pending changes
    Status {
        /// Domain source directory
        #[arg(short, long, default_value = "src/domain")]
        domain: PathBuf,
    },

    /// Generate or update the schema lock file
    Sync {
        /// Domain source directory
        #[arg(short, long, default_value = "src/domain")]
        domain: PathBuf,

        /// Force sync even with breaking changes (dangerous!)
        #[arg(long)]
        force: bool,
    },

    /// Show diff between current code and lock file
    Diff {
        /// Domain source directory
        #[arg(short, long, default_value = "src/domain")]
        domain: PathBuf,
    },

    /// Reset lock file (requires explicit confirmation)
    Reset {
        /// Domain source directory
        #[arg(short, long, default_value = "src/domain")]
        domain: PathBuf,

        /// I understand this may cause data issues
        #[arg(long)]
        i_know_what_im_doing: bool,
    },
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        // No command = launch TUI
        None => {
            // Check if we're in a TTY
            if atty::is(atty::Stream::Stdout) {
                tui::run().await?;
            } else {
                // Non-interactive: show help
                eprintln!("Run 'spitestack --help' for usage or 'spitestack' in a terminal for TUI mode.");
                std::process::exit(1);
            }
        }

        Some(Commands::Init {
            path,
            domain,
            language,
            port,
        }) => {
            init_project(&path, &domain, &language, port).await?;
        }

        Some(Commands::Compile {
            domain,
            output,
            language,
            skip_purity_check,
            port,
        }) => {
            compile_project(&domain, &output, &language, skip_purity_check, port).await?;
        }

        Some(Commands::Check { domain, language }) => {
            let spinner = ui::spinner("Checking domain logic...");

            let config = CompilerConfig {
                domain_dir: domain.clone(),
                out_dir: PathBuf::new(),
                skip_purity_check: false,
                language: language.clone(),
            };

            let compiler = Compiler::new(config);

            // Get stats by parsing
            let mut frontend = spite_compiler::frontend::create_frontend(&language)
                .map_err(|e| miette::miette!("{}", e))?;
            let domain_ir = frontend.parse_directory(&domain)
                .map_err(|e| miette::miette!("{}", e))?;

            match compiler.check().await {
                Ok(_) => {
                    spinner.finish_and_clear();
                    ui::looking_good();
                    println!();
                    println!(
                        "    {} aggregates {} {} commands {} {} events",
                        domain_ir.aggregates.len(),
                        ui::symbols::DOT,
                        domain_ir.aggregates.iter().map(|a| a.commands.len()).sum::<usize>(),
                        ui::symbols::DOT,
                        domain_ir.aggregates.iter().map(|a| a.events.variants.len()).sum::<usize>()
                    );
                    println!("    All pure. No side effects. Ready to ship.");
                }
                Err(e) => {
                    spinner.finish_and_clear();
                    ui::nope_header();
                    return Err(miette::miette!("{}", e));
                }
            }
        }

        Some(Commands::Dev {
            domain,
            output,
            language,
            port,
            skip_purity_check,
        }) => {
            run_dev_mode(&domain, &output, &language, port, skip_purity_check).await?;
        }

        Some(Commands::Watch {
            domain,
            output,
            language,
        }) => {
            run_watch_mode(&domain, &output, &language).await?;
        }

        Some(Commands::Schema { action }) => {
            handle_schema_command(action).await?;
        }
    }

    Ok(())
}

/// Initialize a new SpiteStack project.
async fn init_project(
    path: &PathBuf,
    domain: &std::path::Path,
    _language: &str,
    _port: u16,
) -> miette::Result<()> {
    // Print header
    ui::box_header(&format!("{} Scaffolding your new project", ui::symbols::DIAMOND));
    ui::box_line("");
    ui::box_footer();
    println!();

    let spinner = ui::spinner("Creating project structure...");

    // Create project directory
    std::fs::create_dir_all(path).map_err(|e| miette::miette!("Failed to create project directory: {}", e))?;

    // Create domain directory with example Todo aggregate
    let domain_path = path.join(domain);
    let todo_path = domain_path.join("Todo");
    std::fs::create_dir_all(&todo_path).map_err(|e| miette::miette!("Failed to create domain directory: {}", e))?;

    // Create example TypeScript files
    let events_ts = r#"export type TodoEvent =
  | { type: "Created"; id: string; title: string }
  | { type: "Completed"; completedAt: string }
  | { type: "TitleUpdated"; title: string };
"#;

    let state_ts = r#"export type TodoState = {
  id: string;
  title: string;
  completed: boolean;
  completedAt: string | undefined;
};
"#;

    let aggregate_ts = r#"import { TodoEvent } from "./events";
import { TodoState } from "./state";

export class TodoAggregate {
  static readonly initialState: TodoState = {
    id: "",
    title: "",
    completed: false,
    completedAt: undefined,
  };

  readonly events: TodoEvent[] = [];
  private state: TodoState;

  constructor(initialState: TodoState = TodoAggregate.initialState) {
    this.state = { ...initialState };
  }

  get currentState(): TodoState {
    return this.state;
  }

  protected emit(event: TodoEvent): void {
    this.events.push(event);
    this.apply(event);
  }

  apply(event: TodoEvent): void {
    switch (event.type) {
      case "Created":
        this.state.id = event.id;
        this.state.title = event.title;
        break;
      case "Completed":
        this.state.completed = true;
        this.state.completedAt = event.completedAt;
        break;
      case "TitleUpdated":
        this.state.title = event.title;
        break;
    }
  }

  // Commands
  create(id: string, title: string): void {
    if (!title) {
      throw new Error("Title is required");
    }
    this.emit({ type: "Created", id, title });
  }

  complete(): void {
    if (this.state.completed) {
      throw new Error("Already completed");
    }
    this.emit({ type: "Completed", completedAt: new Date().toISOString() });
  }

  updateTitle(title: string): void {
    if (!title) {
      throw new Error("Title is required");
    }
    this.emit({ type: "TitleUpdated", title });
  }
}
"#;

    std::fs::write(todo_path.join("events.ts"), events_ts)
        .map_err(|e| miette::miette!("Failed to write events.ts: {}", e))?;
    std::fs::write(todo_path.join("state.ts"), state_ts)
        .map_err(|e| miette::miette!("Failed to write state.ts: {}", e))?;
    std::fs::write(todo_path.join("aggregate.ts"), aggregate_ts)
        .map_err(|e| miette::miette!("Failed to write aggregate.ts: {}", e))?;

    spinner.finish_and_clear();

    ui::success("Done. Here's what you got:");
    println!();

    // File tree
    ui::tree_dir("", &domain_path.to_string_lossy());
    println!("      {}{}", console::style("╰─▸ ").dim(), console::style("Todo/").cyan().bold());
    ui::tree_item("          ", "events.ts", Some("What can happen"), false);
    ui::tree_item("          ", "state.ts", Some("What you're tracking"), false);
    ui::tree_item("          ", "aggregate.ts", Some("Your business logic"), true);

    ui::divider();

    println!("  Now do this:");
    println!();
    println!("    cd {}", path.display());
    println!("    spitestack dev");
    println!();
    ui::info("Docs (if you need 'em): https://spitestack.dev/docs");
    println!();

    Ok(())
}

/// Compile domain logic to a TypeScript project.
async fn compile_project(
    domain: &std::path::Path,
    output: &std::path::Path,
    language: &str,
    skip_purity_check: bool,
    port: u16,
) -> miette::Result<()> {
    let start = Instant::now();

    // Derive project name from domain directory
    let project_name = domain
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "spitestack-app".to_string());

    let spinner = ui::spinner("Compiling domain logic...");

    let config = CompilerConfig {
        domain_dir: domain.to_path_buf(),
        out_dir: output.to_path_buf(),
        skip_purity_check,
        language: language.to_string(),
    };

    let compiler = Compiler::new(config);
    compiler.compile_project(&project_name, port).await?;

    spinner.finish_and_clear();

    // Success celebration
    ui::success_banner();

    // Aggregate summary
    ui::box_header("AGGREGATES");
    ui::box_line("");

    // Get aggregate details for the display
    let mut frontend = spite_compiler::frontend::create_frontend(language)
        .map_err(|e| miette::miette!("{}", e))?;
    let domain_ir = frontend.parse_directory(domain)
        .map_err(|e| miette::miette!("{}", e))?;

    let max_events = domain_ir
        .aggregates
        .iter()
        .map(|a| a.events.variants.len())
        .max()
        .unwrap_or(1);

    for agg in &domain_ir.aggregates {
        ui::aggregate_line(
            &agg.name,
            agg.commands.len(),
            agg.events.variants.len(),
            max_events,
        );
    }

    ui::box_line("");
    ui::box_footer();
    println!();

    // Timing
    let duration = start.elapsed().as_millis();
    ui::timing("Done", duration);
    println!();

    // Next steps
    ui::box_header(&format!("{} What's Next", ui::symbols::ARROW));
    ui::box_line("");
    ui::box_line(&format!("   cd {} && bun run dev", output.display()));
    ui::box_line("");
    ui::box_line(&format!("   Then open http://localhost:{}", port));
    ui::box_line("");
    ui::box_footer();
    println!();

    Ok(())
}

/// Run dev mode with hot reload.
async fn run_dev_mode(
    domain: &std::path::Path,
    output: &std::path::Path,
    language: &str,
    port: u16,
    skip_purity_check: bool,
) -> miette::Result<()> {
    // Print dev server banner
    println!();
    println!(
        "{}",
        ui::gradient_text("╔═══════════════════════════════════════════════════════════════════╗")
    );
    println!(
        "{}",
        ui::gradient_text("║                                                                   ║")
    );
    println!(
        "{}",
        ui::gradient_text("║   ◆  S P I T E S T A C K   D E V   S E R V E R                   ║")
    );
    println!(
        "{}",
        ui::gradient_text("║                                                                   ║")
    );
    println!(
        "{}",
        ui::gradient_text("╚═══════════════════════════════════════════════════════════════════╝")
    );
    println!();

    // Initial compile (quiet mode for dev)
    let spinner = ui::spinner("Initial compile...");

    let project_name = domain
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "spitestack-app".to_string());

    let config = CompilerConfig {
        domain_dir: domain.to_path_buf(),
        out_dir: output.to_path_buf(),
        skip_purity_check,
        language: language.to_string(),
    };

    let compiler = Compiler::new(config);
    compiler.compile_project(&project_name, port).await?;

    spinner.finish_and_clear();
    // Channel for file change events
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

    // Set up file watcher
    let domain_path = domain.to_path_buf();
    let tx_clone = tx.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (notify_tx, notify_rx) = std::sync::mpsc::channel();

            let mut debouncer = new_debouncer(
                Duration::from_millis(500),
                move |result: DebounceEventResult| {
                    if let Ok(events) = result {
                        if !events.is_empty() {
                            let _ = notify_tx.send(());
                        }
                    }
                },
            ).expect("Failed to create file watcher");

            debouncer
                .watcher()
                .watch(&domain_path, RecursiveMode::Recursive)
                .expect("Failed to watch directory");

            loop {
                if notify_rx.recv().is_ok() {
                    let _ = tx_clone.try_send(());
                }
            }
        });
    });

    // Start cargo run
    let output_path = output.to_path_buf();

    // Start server
    ui::info(&format!("Server starting on http://localhost:{}", port));
    let mut cargo_process = start_bun_dev(&output_path).await.ok();

    println!();
    ui::info("Ready! Waiting for changes...");

    // Watch for changes
    let domain_clone = domain.to_path_buf();
    let output_clone = output.to_path_buf();
    let language_clone = language.to_string();

    loop {
        tokio::select! {
            _ = rx.recv() => {
                println!();
                ui::box_header(&format!("{} REBUILD", ui::symbols::ARROW));

                // Kill existing process
                if let Some(mut proc) = cargo_process.take() {
                    let _ = proc.kill().await;
                }

                let start = Instant::now();

                // Recompile
                let config = CompilerConfig {
                    domain_dir: domain_clone.clone(),
                    out_dir: output_clone.clone(),
                    skip_purity_check,
                    language: language_clone.clone(),
                };

                let compiler = Compiler::new(config);
                match compiler.recompile_domain().await {
                    Ok(result) => {
                        let duration = start.elapsed().as_millis();
                        ui::box_line("");
                        ui::box_line(&format!(
                            "   {} Compiled {} aggregate(s) in {}ms",
                            ui::symbols::TARGET_FILLED,
                            result.aggregates,
                            duration
                        ));
                        ui::box_line(&format!(
                            "   {} Server hot-reloaded",
                            ui::symbols::TARGET_FILLED
                        ));
                        ui::box_line("");
                        ui::box_footer();

                        // Restart server
                        cargo_process = start_bun_dev(&output_clone).await.ok();
                    }
                    Err(e) => {
                        ui::box_line("");
                        ui::box_line(&format!(
                            "   {} {}",
                            ui::symbols::DIAMOND,
                            e
                        ));
                        ui::box_line("");
                        ui::box_footer();
                    }
                }

                println!();
                ui::info("Ready! Waiting for changes...");
            }
            _ = tokio::signal::ctrl_c() => {
                println!();
                ui::dim("Shutting down...");
                if let Some(mut proc) = cargo_process.take() {
                    let _ = proc.kill().await;
                }
                break;
            }
        }
    }

    Ok(())
}

/// Run watch mode without starting the server.
async fn run_watch_mode(
    domain: &std::path::Path,
    output: &std::path::Path,
    language: &str,
) -> miette::Result<()> {
    ui::info(&format!("Watching for changes in {}", domain.display()));
    println!();

    // Channel for file change events
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

    // Set up file watcher
    let domain_path = domain.to_path_buf();
    let tx_clone = tx.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (notify_tx, notify_rx) = std::sync::mpsc::channel();

            let mut debouncer = new_debouncer(
                Duration::from_millis(500),
                move |result: DebounceEventResult| {
                    if let Ok(events) = result {
                        if !events.is_empty() {
                            let _ = notify_tx.send(());
                        }
                    }
                },
            ).expect("Failed to create file watcher");

            debouncer
                .watcher()
                .watch(&domain_path, RecursiveMode::Recursive)
                .expect("Failed to watch directory");

            loop {
                if notify_rx.recv().is_ok() {
                    let _ = tx_clone.try_send(());
                }
            }
        });
    });

    let domain_clone = domain.to_path_buf();
    let output_clone = output.to_path_buf();
    let language_clone = language.to_string();

    ui::info("Ready! Waiting for changes...");

    loop {
        tokio::select! {
            _ = rx.recv() => {
                println!();
                let spinner = ui::spinner("Change detected, recompiling...");
                let start = Instant::now();

                let config = CompilerConfig {
                    domain_dir: domain_clone.clone(),
                    out_dir: output_clone.clone(),
                    skip_purity_check: false,
                    language: language_clone.clone(),
                };

                let compiler = Compiler::new(config);
                match compiler.recompile_domain().await {
                    Ok(result) => {
                        spinner.finish_and_clear();
                        let duration = start.elapsed().as_millis();
                        ui::success(&format!(
                            "Compiled {} aggregate(s) in {}ms",
                            result.aggregates,
                            duration
                        ));
                    }
                    Err(e) => {
                        spinner.finish_and_clear();
                        ui::error(&format!("{}", e));
                    }
                }
                println!();
                ui::info("Ready! Waiting for changes...");
            }
            _ = tokio::signal::ctrl_c() => {
                println!();
                ui::dim("Stopping watch mode.");
                break;
            }
        }
    }

    Ok(())
}

/// Start bun dev as a background process.
async fn start_bun_dev(project_dir: &PathBuf) -> miette::Result<Child> {
    // First run bun install to ensure dependencies are installed
    let install_status = tokio::process::Command::new("bun")
        .args(["install"])
        .current_dir(project_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .map_err(|e| miette::miette!("Failed to run bun install: {}", e))?;

    if !install_status.success() {
        return Err(miette::miette!("bun install failed"));
    }

    // Then start the dev server
    let child = Command::new("bun")
        .args(["run", "dev"])
        .current_dir(project_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| miette::miette!("Failed to start bun run dev: {}", e))?;

    Ok(child)
}

/// Handle schema management commands.
async fn handle_schema_command(action: SchemaAction) -> miette::Result<()> {
    match action {
        SchemaAction::Status { domain } => {
            schema_status(&domain).await?;
        }
        SchemaAction::Sync { domain, force } => {
            schema_sync(&domain, force).await?;
        }
        SchemaAction::Diff { domain } => {
            schema_diff(&domain).await?;
        }
        SchemaAction::Reset { domain, i_know_what_im_doing } => {
            schema_reset(&domain, i_know_what_im_doing).await?;
        }
    }

    Ok(())
}

/// Show schema status.
async fn schema_status(domain: &PathBuf) -> miette::Result<()> {
    use spite_compiler::schema::SchemaLockFile;
    use spite_compiler::ir::AppMode;

    let spinner = ui::spinner("Loading schema information...");

    // Parse domain to get app config
    let mut frontend = spite_compiler::frontend::create_frontend("typescript")
        .map_err(|e| miette::miette!("{}", e))?;
    let domain_ir = frontend.parse_directory(domain)
        .map_err(|e| miette::miette!("{}", e))?;

    // Parse app config
    let app_config = spite_compiler::frontend::typescript::app_parser::parse_app_config(domain)
        .map_err(|e| miette::miette!("{}", e))?;

    let mode = app_config.as_ref().map(|c| c.mode).unwrap_or(AppMode::Greenfield);

    // Load lock file
    let lock_path = domain.parent().unwrap_or(domain).join("events.lock.json");
    let lock_file = SchemaLockFile::load(&lock_path)
        .map_err(|e| miette::miette!("{}", e))?;

    spinner.finish_and_clear();

    println!();
    ui::box_header(&format!("{} Schema Status", ui::symbols::DIAMOND));
    ui::box_line("");
    ui::box_line(&format!("Mode: {}", match mode {
        AppMode::Greenfield => "greenfield (schemas can change freely)",
        AppMode::Production => "production (schemas are locked)",
    }));
    ui::box_line(&format!("Lock file: {}", if lock_file.is_some() {
        lock_path.display().to_string()
    } else {
        "not found".to_string()
    }));
    ui::box_line("");

    // Show aggregates
    ui::box_line("Aggregates:");
    for agg in &domain_ir.aggregates {
        let event_count = agg.events.variants.len();
        ui::box_line(&format!("  {} ({} events)", agg.name, event_count));
    }
    ui::box_line("");

    // If lock file exists and in production mode, check for changes
    if let Some(ref locked) = lock_file {
        if mode == AppMode::Production {
            let diffs = spite_compiler::schema::diff_schemas(&locked.aggregates, &domain_ir);
            if diffs.is_empty() {
                ui::box_line("Status: No schema changes detected");
            } else {
                let breaking_count = diffs.iter().filter(|d| d.is_breaking()).count();
                let safe_count = diffs.iter().filter(|d| d.can_auto_upcast()).count();

                if breaking_count > 0 {
                    ui::box_line(&format!("Status: {} breaking change(s) detected!", breaking_count));
                    ui::box_line("        Run `spitestack schema diff` for details");
                } else if safe_count > 0 {
                    ui::box_line(&format!("Status: {} non-breaking change(s) detected", safe_count));
                    ui::box_line("        These will be auto-upcasted on next compile");
                }
            }
        }
    } else if mode == AppMode::Production {
        ui::box_line("Status: Lock file required for production mode");
        ui::box_line("        Run `spitestack schema sync` to generate");
    }

    ui::box_footer();
    println!();

    Ok(())
}

/// Generate or update the schema lock file.
async fn schema_sync(domain: &PathBuf, force: bool) -> miette::Result<()> {
    use spite_compiler::schema::SchemaLockFile;

    let spinner = ui::spinner("Syncing schema...");

    // Parse domain
    let mut frontend = spite_compiler::frontend::create_frontend("typescript")
        .map_err(|e| miette::miette!("{}", e))?;
    let domain_ir = frontend.parse_directory(domain)
        .map_err(|e| miette::miette!("{}", e))?;

    // Load existing lock file
    let lock_path = domain.parent().unwrap_or(domain).join("events.lock.json");
    let existing = SchemaLockFile::load(&lock_path)
        .map_err(|e| miette::miette!("{}", e))?;

    // Check for breaking changes if lock file exists
    if let Some(ref locked) = existing {
        let diffs = spite_compiler::schema::diff_schemas(&locked.aggregates, &domain_ir);
        let breaking = diffs.iter().filter(|d| d.is_breaking()).collect::<Vec<_>>();

        if !breaking.is_empty() && !force {
            spinner.finish_and_clear();
            ui::nope_header();
            println!();
            println!("  Breaking changes detected:");
            for diff in &breaking {
                println!("    {}.{}", diff.aggregate, diff.event);
                println!("{}", diff.format_changes());
            }
            println!();
            println!("  Use --force to sync anyway (WARNING: may break event replay)");
            return Err(miette::miette!("Breaking changes detected"));
        }
    }

    // Generate and save new lock file
    let lock = SchemaLockFile::from_domain_ir(&domain_ir, env!("CARGO_PKG_VERSION"));
    lock.save(&lock_path)
        .map_err(|e| miette::miette!("{}", e))?;

    spinner.finish_and_clear();
    ui::looking_good();
    println!();
    println!("    Schema lock file updated: {}", lock_path.display());
    println!("    {} aggregates, {} events captured",
        lock.aggregates.len(),
        lock.aggregates.values().map(|a| a.events.len()).sum::<usize>()
    );

    Ok(())
}

/// Show diff between current code and lock file.
async fn schema_diff(domain: &PathBuf) -> miette::Result<()> {
    use spite_compiler::schema::SchemaLockFile;

    let spinner = ui::spinner("Comparing schemas...");

    // Parse domain
    let mut frontend = spite_compiler::frontend::create_frontend("typescript")
        .map_err(|e| miette::miette!("{}", e))?;
    let domain_ir = frontend.parse_directory(domain)
        .map_err(|e| miette::miette!("{}", e))?;

    // Load lock file
    let lock_path = domain.parent().unwrap_or(domain).join("events.lock.json");
    let locked = SchemaLockFile::load(&lock_path)
        .map_err(|e| miette::miette!("{}", e))?;

    spinner.finish_and_clear();

    match locked {
        None => {
            println!();
            println!("  No lock file found at {}", lock_path.display());
            println!("  Run `spitestack schema sync` to generate one");
        }
        Some(locked) => {
            let diffs = spite_compiler::schema::diff_schemas(&locked.aggregates, &domain_ir);

            if diffs.is_empty() {
                ui::looking_good();
                println!();
                println!("    No schema changes detected");
            } else {
                println!();
                ui::box_header(&format!("{} Schema Changes", ui::symbols::TRIANGLE));
                ui::box_line("");

                for diff in &diffs {
                    let marker = if diff.is_breaking() { "BREAKING" } else { "OK" };
                    ui::box_line(&format!("{}.{} [{}]", diff.aggregate, diff.event, marker));
                    for line in diff.format_changes().lines() {
                        ui::box_line(line);
                    }
                    ui::box_line("");
                }

                ui::box_footer();
            }
        }
    }

    Ok(())
}

/// Reset the schema lock file.
async fn schema_reset(domain: &PathBuf, confirmed: bool) -> miette::Result<()> {
    let lock_path = domain.parent().unwrap_or(domain).join("events.lock.json");

    if !confirmed {
        ui::nope_header();
        println!();
        println!("  This will delete the schema lock file.");
        println!("  Existing events in the database may not replay correctly");
        println!("  after you regenerate the lock file with different schemas.");
        println!();
        println!("  If you're sure, run:");
        println!("    spitestack schema reset --i-know-what-im-doing");
        return Ok(());
    }

    if lock_path.exists() {
        std::fs::remove_file(&lock_path)
            .map_err(|e| miette::miette!("Failed to delete lock file: {}", e))?;

        println!();
        println!("  Lock file deleted: {}", lock_path.display());
        println!("  Run `spitestack schema sync` to generate a new one");
    } else {
        println!();
        println!("  No lock file found at {}", lock_path.display());
    }

    Ok(())
}
