mod cursor;
mod log;
mod outline;
mod server;
mod update_check;

use clap::Parser;
use rmcp::ServiceExt;
use rmcp::transport::stdio;

use server::FfrServer;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser, Debug)]
#[command(name = "ffr-mcp", about = "ffr file reading engine — MCP server")]
struct Args {
    /// Chunk size in bytes for read_chunk operations
    #[arg(long, default_value_t = 64 * 1024)]
    chunk_bytes: usize,

    /// Binary sniff buffer size
    #[arg(long, default_value_t = 4096)]
    sniff_bytes: usize,

    /// Maximum file size for full-text open (bytes)
    #[arg(long, default_value_t = 2 * 1024 * 1024)]
    full_open_max_bytes: u64,

    /// Line length threshold for minified file detection
    #[arg(long, default_value_t = 1000)]
    minified_threshold: usize,

    /// Path for persistent metadata cache (LMDB dir, or legacy .json auto-migrated)
    #[arg(long)]
    metadata_cache_path: Option<String>,

    /// Tracing log file (default: $XDG_CACHE_HOME/ffr/ffr-mcp.log)
    #[arg(long)]
    log_file: Option<String>,

    /// Tracing log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Skip the one-time background GitHub release version check
    #[arg(long, default_value_t = false)]
    no_update_check: bool,

    /// Run diagnostics (LMDB open, classify on self-binary, tool catalogue) and exit.
    #[arg(long, default_value_t = false)]
    healthcheck: bool,
}

fn default_log_file() -> String {
    let base = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
    base.join("ffr").join("ffr-mcp.log").to_string_lossy().into_owned()
}

/// Exit code: 0 ok, 1 critical failure (LMDB unreachable, classify errors).
fn run_healthcheck(args: &Args) -> i32 {
    let mut status = 0i32;
    println!("ffr-mcp healthcheck");
    println!("  version: {}", env!("CARGO_PKG_VERSION"));
    println!("  log_file: {}", args.log_file.clone().unwrap_or_else(default_log_file));
    println!("  log_level: {}", args.log_level);
    println!("  chunk_bytes: {}", args.chunk_bytes);
    println!("  sniff_bytes: {}", args.sniff_bytes);
    println!("  full_open_max_bytes: {}", args.full_open_max_bytes);

    // LMDB open
    if let Some(ref path) = args.metadata_cache_path {
        match ffr_core::cache::load_metadata_index(path) {
            Ok(()) => {
                let n = ffr_core::cache::metadata_count().unwrap_or(0);
                let p = ffr_core::cache::metadata_path().ok().flatten().unwrap_or_default();
                println!("  [ok] metadata LMDB: {p} ({n} entries)");
            }
            Err(e) => {
                println!("  [err] metadata LMDB open {path}: {e}");
                status = 1;
            }
        }
    } else {
        println!("  [info] metadata_cache_path not provided; skipping LMDB check");
    }

    // Classify the current executable as a known-good file.
    match std::env::current_exe() {
        Ok(p) => {
            let ps = p.to_string_lossy();
            match ffr_core::classify::classify_path(&ps, args.sniff_bytes, args.full_open_max_bytes, args.minified_threshold) {
                Ok(r) => {
                    println!("  [ok] classify({ps}) → kind={}, binary={}", r.kind, r.binary);
                }
                Err(e) => {
                    println!("  [err] classify({ps}): {e}");
                    status = 1;
                }
            }
        }
        Err(e) => {
            println!("  [warn] current_exe unavailable: {e}");
        }
    }

    // Tool catalogue — self-describing count.
    println!("  [ok] tools: {}", server::tool_catalog_size());

    if status == 0 {
        println!("  OK");
    } else {
        println!("  FAILED");
    }
    status
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.healthcheck {
        std::process::exit(run_healthcheck(&args));
    }

    let log_file = args.log_file.clone().unwrap_or_else(default_log_file);
    if let Err(e) = ffr_core::log::init_tracing(&log_file, Some(&args.log_level)) {
        eprintln!("ffr-mcp: init_tracing failed: {e}");
    }

    if !args.no_update_check {
        update_check::spawn_update_check();
    }

    if let Some(ref path) = args.metadata_cache_path {
        if let Err(e) = ffr_core::cache::load_metadata_index(path) {
            tracing::warn!(error = %e, path = %path, "load_metadata_index failed");
        }
    }

    let server = FfrServer::new(
        args.chunk_bytes,
        args.sniff_bytes,
        args.full_open_max_bytes,
        args.minified_threshold,
    );

    let service = server
        .serve(stdio())
        .await
        .map_err(|e| format!("Failed to start MCP server: {e}"))?;

    service.waiting().await?;

    let _ = ffr_core::cache::save_metadata_index();

    Ok(())
}
