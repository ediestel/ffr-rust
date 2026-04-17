//! Tracing initialization for ffr crates.
//!
//! Single log file, non-blocking writer, truncates on setup. Callers:
//! - `ffr-nvim::lua_configure` passes `stdpath('data')/ffr/ffr.log`.
//! - `ffr-mcp::main` passes `XDG_CACHE_HOME/ffr/ffr-mcp.log` by default.

use std::io;
use std::path::Path;
use std::sync::OnceLock;

use tracing_appender::non_blocking;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

static TRACING_GUARD: OnceLock<WorkerGuard> = OnceLock::new();
static PANIC_HOOK: OnceLock<()> = OnceLock::new();

/// Parse a log level string into a `tracing::Level`. Unknown values → INFO.
pub fn parse_log_level(level: Option<&str>) -> tracing::Level {
    match level.as_ref().map(|s| s.trim().to_lowercase()).as_deref() {
        Some("trace") => tracing::Level::TRACE,
        Some("debug") => tracing::Level::DEBUG,
        Some("info") => tracing::Level::INFO,
        Some("warn") => tracing::Level::WARN,
        Some("error") => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    }
}

/// Install a panic hook that logs to tracing and appends to a fallback file
/// under `dirs::cache_dir()/ffr/panic.log`. Idempotent.
pub fn install_panic_hook() {
    PANIC_HOOK.get_or_init(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let msg = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            let loc = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "unknown".to_string());

            tracing::error!(panic.message = %msg, panic.location = %loc, "ffr panic");
            eprintln!("=== FFR PANIC ===\n{loc}: {msg}\n=================");

            if let Some(cache) = dirs::cache_dir() {
                let panic_log = cache.join("ffr").join("panic.log");
                if let Some(parent) = panic_log.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let entry = format!("\n[{ts}] {loc}: {msg}\n");
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&panic_log)
                    .and_then(|mut f| {
                        use std::io::Write;
                        f.write_all(entry.as_bytes())
                    });
            }

            default_hook(info);
        }));
    });
}

/// Initialize tracing once. Subsequent calls are no-ops (the initial
/// configuration wins). Returns the resolved log file path on success.
pub fn init_tracing(log_file_path: &str, log_level: Option<&str>) -> Result<String, io::Error> {
    install_panic_hook();

    let log_path = Path::new(log_file_path);
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_path)?;

    let level = parse_log_level(log_level);

    TRACING_GUARD.get_or_init(|| {
        let (writer, guard) = non_blocking(file);
        let subscriber = tracing_subscriber::registry()
            .with(
                fmt::layer()
                    .with_writer(writer)
                    .with_target(true)
                    .with_ansi(false)
                    .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE),
            )
            .with(
                EnvFilter::builder()
                    .with_default_directive(level.into())
                    .from_env_lossy(),
            );

        if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
            eprintln!("ffr: failed to set tracing subscriber: {e}");
        } else {
            tracing::info!(log = %log_path.display(), level = ?level, "ffr tracing initialized");
        }
        guard
    });

    Ok(log_file_path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_levels() {
        assert_eq!(parse_log_level(Some("debug")), tracing::Level::DEBUG);
        assert_eq!(parse_log_level(Some("TRACE")), tracing::Level::TRACE);
        assert_eq!(parse_log_level(Some("nope")), tracing::Level::INFO);
        assert_eq!(parse_log_level(None), tracing::Level::INFO);
    }

    #[test]
    fn init_tracing_creates_parent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let log = tmp.path().join("sub").join("ffr.log");
        let got = init_tracing(log.to_str().unwrap(), Some("info")).unwrap();
        assert_eq!(got, log.to_str().unwrap());
        assert!(log.exists());
    }
}
