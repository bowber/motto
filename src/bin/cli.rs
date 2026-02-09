//! Motto CLI - Code generation tool for multi-platform SDKs

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use motto::prelude::*;
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "motto")]
#[command(author, version, about = "Generate multi-platform SDKs from Rust schema", long_about = None)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new motto project
    Init(InitArgs),

    /// Generate SDK code from schema.rs
    Generate(GenerateArgs),

    /// Check schema for breaking changes
    Check(CheckArgs),

    /// Update motto.lock with new schema fingerprint
    Lock(LockArgs),

    /// Watch for schema changes and regenerate
    Watch(WatchArgs),

    /// Sniff transport traffic for debugging
    Sniff(SniffArgs),
}

#[derive(Args)]
struct InitArgs {
    /// Project directory (defaults to current directory)
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Template to use (minimal, full)
    #[arg(short, long, default_value = "minimal")]
    template: String,
}

#[derive(Args)]
struct GenerateArgs {
    /// Path to schema.rs file
    #[arg(short, long, default_value = "src/schema.rs")]
    schema: PathBuf,

    /// Output directory for generated code
    #[arg(short, long, default_value = "generated")]
    output: PathBuf,

    /// Target platforms to generate (comma-separated)
    /// Options: typescript, swift, kotlin, unity, rust, all
    #[arg(short, long, default_value = "all")]
    targets: String,

    /// Force regeneration even if fingerprint matches
    #[arg(short, long)]
    force: bool,

    /// Generate WASM bindings for TypeScript target
    #[arg(long)]
    wasm: bool,

    /// Generate native addon bindings (napi-rs) for TypeScript target
    #[arg(long)]
    native: bool,

    /// Transport mode for generated SDKs: runtime (pure-language, default) or ffi (shared Rust core)
    #[arg(long, default_value = "runtime")]
    transport: String,
}

#[derive(Args)]
struct CheckArgs {
    /// Path to schema.rs file
    #[arg(short, long, default_value = "src/schema.rs")]
    schema: PathBuf,

    /// Path to motto.lock file
    #[arg(short, long, default_value = "motto.lock")]
    lock: PathBuf,

    /// Strict mode: treat warnings as errors
    #[arg(long)]
    strict: bool,
}

#[derive(Args)]
struct LockArgs {
    /// Path to schema.rs file
    #[arg(short, long, default_value = "src/schema.rs")]
    schema: PathBuf,

    /// Path to motto.lock file
    #[arg(short, long, default_value = "motto.lock")]
    lock: PathBuf,

    /// Bump version (major, minor, patch)
    #[arg(short, long, default_value = "patch")]
    bump: String,
}

#[derive(Args)]
struct WatchArgs {
    /// Path to schema.rs file
    #[arg(short, long, default_value = "src/schema.rs")]
    schema: PathBuf,

    /// Output directory for generated code
    #[arg(short, long, default_value = "generated")]
    output: PathBuf,

    /// Target platforms to generate
    #[arg(short, long, default_value = "all")]
    targets: String,
}

#[derive(Args)]
struct SniffArgs {
    /// Sniff mode: tap (listen to FFI transport events) or proxy (WS relay with frame logging)
    #[arg(short, long, default_value = "tap")]
    mode: String,

    /// Output format: pretty (human-readable), json, or hex
    #[arg(short, long, default_value = "pretty")]
    format: String,

    /// Path to schema.rs for protocol-aware decoding
    #[arg(short, long, default_value = "src/schema.rs")]
    schema: PathBuf,

    /// Disable schema-aware packet decoding
    #[arg(long)]
    no_decode: bool,

    /// Redact field values in output (show structure only)
    #[arg(long)]
    redact: bool,

    /// Maximum bytes to display per packet (0 = unlimited)
    #[arg(long, default_value = "0")]
    max_bytes: usize,

    /// Proxy listen address (only used in proxy mode)
    #[arg(long, default_value = "127.0.0.1:9901")]
    listen: String,

    /// Proxy upstream address (only used in proxy mode)
    #[arg(long)]
    upstream: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let level = if cli.verbose {
        Level::DEBUG
    } else {
        Level::INFO
    };
    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber).context("Failed to set up logging")?;

    match cli.command {
        Commands::Init(args) => cmd_init(args),
        Commands::Generate(args) => cmd_generate(args),
        Commands::Check(args) => cmd_check(args),
        Commands::Lock(args) => cmd_lock(args),
        Commands::Watch(args) => cmd_watch(args),
        Commands::Sniff(args) => cmd_sniff(args),
    }
}

fn cmd_init(args: InitArgs) -> Result<()> {
    let path = args.path.unwrap_or_else(|| PathBuf::from("."));
    info!(
        "Initializing motto project in {:?} with template '{}'",
        path, args.template
    );

    // Create directory structure
    std::fs::create_dir_all(path.join("src"))?;
    std::fs::create_dir_all(path.join("generated"))?;

    // Create example schema.rs
    let schema_content = include_str!("../templates/example_schema.rs");
    std::fs::write(path.join("src/schema.rs"), schema_content)?;

    // Create initial motto.lock
    let lock_content = motto::ir::lock::MottoLock::new().to_string()?;
    std::fs::write(path.join("motto.lock"), lock_content)?;

    info!("✓ Created motto project structure");
    info!("  - src/schema.rs: Your schema definitions");
    info!("  - motto.lock: Version tracking file");
    info!("  - generated/: Output directory for SDK code");
    info!("\nRun 'motto generate' to create your first SDK");

    Ok(())
}

fn cmd_generate(args: GenerateArgs) -> Result<()> {
    info!("Generating SDKs from {:?}", args.schema);

    // Parse transport mode
    let transport_mode: motto::emitters::TransportMode = args
        .transport
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;
    info!("Transport mode: {}", transport_mode);

    // Parse schema (use parse_file to get proper schema name from file)
    let parser = SchemaParser::new();
    let schema = parser.parse_file(&args.schema)?;

    // Generate fingerprint
    let fingerprint = SchemaFingerprint::compute(&schema);
    info!("Schema fingerprint: {}", fingerprint.short());

    // Generate IR
    let ir_gen = IrGenerator::new();
    let manifest = ir_gen.generate(&schema)?;

    // Parse targets
    let targets: Vec<&str> = if args.targets == "all" {
        #[allow(clippy::vec_init_then_push)]
        let t = {
            let mut t = Vec::new();
            #[cfg(feature = "emitter-typescript")]
            t.push("typescript");
            #[cfg(feature = "emitter-swift")]
            t.push("swift");
            #[cfg(feature = "emitter-kotlin")]
            t.push("kotlin");
            #[cfg(feature = "emitter-unity")]
            t.push("unity");
            t.push("rust");
            t
        };
        t
    } else {
        args.targets.split(',').map(|s| s.trim()).collect()
    };

    // Create output directory
    std::fs::create_dir_all(&args.output)?;

    // Generate for each target
    for target in targets {
        info!("Generating {} SDK...", target);

        let config = motto::emitters::EmitterConfig {
            output_dir: args.output.clone(),
            wasm_bindings: args.wasm,
            native_bindings: args.native,
            transport_mode,
            manifest: manifest.clone(),
        };

        match target {
            #[cfg(feature = "emitter-typescript")]
            "typescript" => motto::emitters::typescript::emit(&config)?,
            #[cfg(feature = "emitter-swift")]
            "swift" => motto::emitters::swift::emit(&config)?,
            #[cfg(feature = "emitter-kotlin")]
            "kotlin" => motto::emitters::kotlin::emit(&config)?,
            #[cfg(feature = "emitter-unity")]
            "unity" => motto::emitters::unity::emit(&config)?,
            "rust" => motto::emitters::rust::emit(&config)?,
            _ => anyhow::bail!(
                "Unknown target: {}. Make sure the corresponding emitter feature is enabled.",
                target
            ),
        }
    }

    info!("✓ SDK generation complete!");
    info!("  Output: {:?}", args.output);

    Ok(())
}

fn cmd_check(args: CheckArgs) -> Result<()> {
    info!("Checking schema compatibility...");

    // Parse current schema
    let parser = SchemaParser::new();
    let schema = parser.parse_file(&args.schema)?;
    let fingerprint = SchemaFingerprint::compute(&schema);

    // Load motto.lock
    let lock_content = std::fs::read_to_string(&args.lock).context("Failed to read motto.lock")?;
    let lock = motto::ir::lock::MottoLock::parse_str(&lock_content)?;

    // Compare fingerprints
    if fingerprint.hash() == lock.fingerprint() {
        info!(
            "✓ Schema matches motto.lock (fingerprint: {})",
            fingerprint.short()
        );
        Ok(())
    } else {
        let msg = format!(
            "Schema fingerprint mismatch!\n  Current: {}\n  Locked:  {}\n\nRun 'motto lock' to update",
            fingerprint.short(),
            &lock.fingerprint()[..16]
        );
        if args.strict {
            anyhow::bail!(msg);
        } else {
            eprintln!("⚠ {}", msg);
            Ok(())
        }
    }
}

fn cmd_sniff(args: SniffArgs) -> Result<()> {
    use motto::runtime::sniff::{OutputFormat, SniffConfig};

    info!("Starting motto sniff in '{}' mode...", args.mode);
    info!("Output format: {}", args.format);

    // Parse output format
    let format: OutputFormat = args
        .format
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    // Load schema manifest for decoding (enabled by default)
    let decode = !args.no_decode;
    let manifest = if decode {
        let schema_path = &args.schema;
        if schema_path.exists() {
            info!("Loading schema from {:?} for decode mode", schema_path);
            let parser = SchemaParser::new();
            match parser.parse_file(schema_path) {
                Ok(schema) => {
                    let ir_gen = IrGenerator::new();
                    match ir_gen.generate(&schema) {
                        Ok(m) => Some(m),
                        Err(e) => {
                            eprintln!(
                                "motto sniff: failed to generate IR from {:?}: {} (decoding disabled)",
                                schema_path, e
                            );
                            None
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "motto sniff: failed to parse {:?}: {} (decoding disabled)",
                        schema_path, e
                    );
                    None
                }
            }
        } else {
            info!(
                "Schema file {:?} not found, running without decode",
                schema_path
            );
            None
        }
    } else {
        None
    };

    let config = SniffConfig {
        format,
        manifest,
        decode,
        redact: args.redact,
        max_bytes: args.max_bytes,
    };

    // Build a tokio runtime for the async sniff operations
    let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;

    match args.mode.as_str() {
        "tap" => {
            // Tap mode requires --upstream (the WS endpoint to connect to and listen on)
            let url = args.upstream.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "tap mode requires --upstream <ws://host:port> to specify the endpoint to listen on"
                )
            })?;
            info!("Tap mode: connecting to {} to listen for frames", url);
            rt.block_on(async {
                motto::runtime::sniff::run_tap(url, &config)
                    .await
                    .map_err(|e| anyhow::anyhow!("sniff tap error: {}", e))
            })
        }
        "proxy" => {
            let upstream = args.upstream.as_deref().ok_or_else(|| {
                anyhow::anyhow!("proxy mode requires --upstream <ws://host:port>")
            })?;
            info!(
                "Proxy mode: {} -> {} (relay with frame logging)",
                args.listen, upstream
            );
            rt.block_on(async {
                motto::runtime::sniff::run_proxy(&args.listen, upstream, &config)
                    .await
                    .map_err(|e| anyhow::anyhow!("sniff proxy error: {}", e))
            })
        }
        other => anyhow::bail!("Unknown sniff mode '{}': expected 'tap' or 'proxy'", other),
    }
}

fn cmd_lock(args: LockArgs) -> Result<()> {
    info!("Updating motto.lock...");

    // Parse schema
    let parser = SchemaParser::new();
    let schema = parser.parse_file(&args.schema)?;
    let fingerprint = SchemaFingerprint::compute(&schema);

    // Load or create motto.lock
    let mut lock = if args.lock.exists() {
        let content = std::fs::read_to_string(&args.lock)?;
        motto::ir::lock::MottoLock::parse_str(&content)?
    } else {
        motto::ir::lock::MottoLock::new()
    };

    // Bump version
    match args.bump.as_str() {
        "major" => lock.bump_major(),
        "minor" => lock.bump_minor(),
        "patch" => lock.bump_patch(),
        _ => anyhow::bail!("Invalid bump type: {}", args.bump),
    }

    // Update fingerprint
    lock.set_fingerprint(fingerprint.hash());

    // Write lock file
    std::fs::write(&args.lock, lock.to_string()?)?;

    info!("✓ Updated motto.lock");
    info!("  Version: {}", lock.version());
    info!("  Fingerprint: {}", fingerprint.short());

    Ok(())
}

fn cmd_watch(args: WatchArgs) -> Result<()> {
    info!("Watching {:?} for changes...", args.schema);
    info!("Press Ctrl+C to stop");

    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc::channel;

    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        Config::default(),
    )?;

    watcher.watch(&args.schema, RecursiveMode::NonRecursive)?;

    loop {
        match rx.recv() {
            Ok(_event) => {
                info!("Schema changed, regenerating...");
                let gen_args = GenerateArgs {
                    schema: args.schema.clone(),
                    output: args.output.clone(),
                    targets: args.targets.clone(),
                    force: true,
                    wasm: false,
                    native: false,
                    transport: "runtime".to_string(),
                };
                if let Err(e) = cmd_generate(gen_args) {
                    eprintln!("Generation error: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Watch error: {}", e);
                break;
            }
        }
    }

    Ok(())
}
