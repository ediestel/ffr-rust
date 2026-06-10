use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use mlua::LuaSerdeExt;
use mlua::prelude::*;

use ffr_core::watcher::FileWatcher;
use ffr_core::{cache, classify, lines, log, prefetch, read, specialized, stat};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// ---------------------------------------------------------------------------
// lua module entry point — called by package.loadlib(path, "luaopen_ffr_nvim")
// ---------------------------------------------------------------------------

#[mlua::lua_module(skip_memory_check)]
fn ffr_nvim(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;

    exports.set("stat_path", lua.create_function(lua_stat_path)?)?;
    exports.set("classify_path", lua.create_function(lua_classify_path)?)?;
    exports.set("read_bytes", lua.create_function(lua_read_bytes)?)?;
    exports.set("read_lines", lua.create_function(lua_read_lines)?)?;
    exports.set(
        "build_line_index",
        lua.create_function(lua_build_line_index)?,
    )?;
    exports.set("read_chunk", lua.create_function(lua_read_chunk)?)?;
    exports.set("configure", lua.create_function(lua_configure)?)?;
    exports.set("shutdown", lua.create_function(lua_shutdown)?)?;

    // Watcher ----------------------------------------------------------------
    exports.set("watcher_spawn", lua.create_function(lua_watcher_spawn)?)?;
    exports.set("watcher_watch", lua.create_function(lua_watcher_watch)?)?;
    exports.set("watcher_unwatch", lua.create_function(lua_watcher_unwatch)?)?;
    exports.set("watcher_status", lua.create_function(lua_watcher_status)?)?;
    exports.set("watcher_stop", lua.create_function(lua_watcher_stop)?)?;

    // Cache admin -----------------------------------------------------------
    exports.set("clear_cache", lua.create_function(lua_clear_cache)?)?;
    exports.set("metadata_info", lua.create_function(lua_metadata_info)?)?;
    exports.set("invalidate_path", lua.create_function(lua_invalidate_path)?)?;

    // Specialized handlers --------------------------------------------------
    exports.set(
        "extract_specialized",
        lua.create_function(lua_extract_specialized)?,
    )?;

    // Prefetch --------------------------------------------------------------
    exports.set("prefetch_spawn", lua.create_function(lua_prefetch_spawn)?)?;
    exports.set("prefetch_hint", lua.create_function(lua_prefetch_hint)?)?;

    // Semantic chunk persistence -------------------------------------------
    exports.set("semantic_get", lua.create_function(lua_semantic_get)?)?;
    exports.set("semantic_upsert", lua.create_function(lua_semantic_upsert)?)?;
    exports.set("semantic_remove", lua.create_function(lua_semantic_remove)?)?;

    exports.set("protocol_version", "1")?;

    Ok(exports)
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn core_err(e: ffr_core::errors::FFRError) -> LuaError {
    LuaError::external(e)
}

fn watcher_handle() -> &'static Mutex<Option<FileWatcher>> {
    static H: OnceLock<Mutex<Option<FileWatcher>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(None))
}

// ---------------------------------------------------------------------------
// exported functions
// ---------------------------------------------------------------------------

fn lua_stat_path(lua: &Lua, path: String) -> LuaResult<LuaValue> {
    let result = stat::stat_path(&path).map_err(core_err)?;
    lua.to_value(&result)
}

fn lua_classify_path(lua: &Lua, args: LuaTable) -> LuaResult<LuaValue> {
    let path: String = args.get("path")?;
    let sniff_bytes: usize = args.get("sniff_bytes")?;
    let full_open_max: u64 = args.get("full_open_max_bytes")?;
    let minified_threshold: usize = args.get("minified_line_length_threshold")?;

    let result = classify::classify_path(&path, sniff_bytes, full_open_max, minified_threshold)
        .map_err(core_err)?;

    lua.to_value(&result)
}

fn lua_read_bytes(lua: &Lua, args: LuaTable) -> LuaResult<LuaValue> {
    let path: String = args.get("path")?;
    let offset: u64 = args.get("offset")?;
    let length: usize = args.get("length")?;

    let result = read::read_bytes(&path, offset, length).map_err(core_err)?;
    lua.to_value(&result)
}

fn lua_read_lines(lua: &Lua, args: LuaTable) -> LuaResult<LuaValue> {
    let path: String = args.get("path")?;
    let start_line: usize = args.get("start_line")?;
    let end_line: usize = args.get("end_line")?;

    let result = lines::read_lines(&path, start_line, end_line).map_err(core_err)?;
    lua.to_value(&result)
}

fn lua_build_line_index(lua: &Lua, path: String) -> LuaResult<LuaValue> {
    let result = lines::build_line_index(&path).map_err(core_err)?;
    lua.to_value(&result)
}

fn lua_read_chunk(lua: &Lua, args: LuaTable) -> LuaResult<LuaValue> {
    let path: String = args.get("path")?;
    let chunk_id: u64 = args.get("chunk_id")?;
    let chunk_bytes: Option<usize> = args.get("chunk_bytes")?;

    let chunk_bytes: usize =
        chunk_bytes.ok_or_else(|| LuaError::RuntimeError("chunk_bytes is required".to_string()))?;
    let result = read::read_chunk(&path, chunk_id, chunk_bytes).map_err(core_err)?;
    lua.to_value(&result)
}

fn lua_configure(lua: &Lua, args: LuaTable) -> LuaResult<LuaValue> {
    let metadata_cache_path: Option<String> = args.get("metadata_cache_path")?;
    let log_path: Option<String> = args.get("log_path").ok();
    let log_level: Option<String> = args.get("log_level").ok();

    if let Some(ref path) = log_path {
        if let Err(e) = log::init_tracing(path, log_level.as_deref()) {
            eprintln!("ffr: init_tracing failed: {e}");
        }
    }

    if let Some(ref path) = metadata_cache_path {
        if let Err(e) = cache::load_metadata_index(path) {
            tracing::warn!(error = %e, path = %path, "load_metadata_index failed");
        }
    }

    let result = lua.create_table()?;
    result.set("ok", true)?;
    result.set("protocol_version", "1")?;
    Ok(LuaValue::Table(result))
}

fn lua_shutdown(_lua: &Lua, _: ()) -> LuaResult<bool> {
    // Stop and drop the watcher before LMDB close so its thread cannot
    // race on the cache handle.
    if let Ok(mut g) = watcher_handle().lock() {
        if let Some(mut w) = g.take() {
            w.stop();
        }
    }
    let _ = cache::save_metadata_index();
    Ok(true)
}

// ---------------------------------------------------------------------------
// watcher FFI
// ---------------------------------------------------------------------------

fn lua_watcher_spawn(_lua: &Lua, args: Option<LuaTable>) -> LuaResult<bool> {
    let debounce_ms: Option<u64> = match &args {
        Some(t) => t.get("debounce_ms").ok(),
        None => None,
    };

    let mut guard = watcher_handle()
        .lock()
        .map_err(|e| LuaError::RuntimeError(format!("watcher lock: {e}")))?;

    if guard.is_some() {
        return Ok(true);
    }

    let debounce = debounce_ms.map(Duration::from_millis);
    let watcher = FileWatcher::spawn(
        cache::shared_metadata(),
        cache::shared_line_index_cache(),
        debounce,
    )
    .map_err(core_err)?;
    *guard = Some(watcher);
    Ok(true)
}

fn lua_watcher_watch(_lua: &Lua, path: String) -> LuaResult<bool> {
    let guard = watcher_handle()
        .lock()
        .map_err(|e| LuaError::RuntimeError(format!("watcher lock: {e}")))?;
    let w = match guard.as_ref() {
        Some(w) => w,
        None => return Ok(false),
    };
    w.watch(std::path::Path::new(&path)).map_err(core_err)?;
    Ok(true)
}

fn lua_watcher_unwatch(_lua: &Lua, path: String) -> LuaResult<bool> {
    let guard = watcher_handle()
        .lock()
        .map_err(|e| LuaError::RuntimeError(format!("watcher lock: {e}")))?;
    let w = match guard.as_ref() {
        Some(w) => w,
        None => return Ok(false),
    };
    w.unwatch(std::path::Path::new(&path)).map_err(core_err)?;
    Ok(true)
}

fn lua_watcher_status(lua: &Lua, _: ()) -> LuaResult<LuaValue> {
    let guard = watcher_handle()
        .lock()
        .map_err(|e| LuaError::RuntimeError(format!("watcher lock: {e}")))?;

    let result = lua.create_table()?;
    match guard.as_ref() {
        Some(w) => {
            result.set("running", true)?;
            let paths: Vec<String> = w
                .watched_paths()
                .into_iter()
                .map(|p: PathBuf| p.to_string_lossy().into_owned())
                .collect();
            let list = lua.create_table()?;
            for (i, p) in paths.iter().enumerate() {
                list.set(i + 1, p.as_str())?;
            }
            result.set("watched", list)?;
        }
        None => {
            result.set("running", false)?;
            result.set("watched", lua.create_table()?)?;
        }
    }
    Ok(LuaValue::Table(result))
}

fn lua_watcher_stop(_lua: &Lua, _: ()) -> LuaResult<bool> {
    let mut guard = watcher_handle()
        .lock()
        .map_err(|e| LuaError::RuntimeError(format!("watcher lock: {e}")))?;
    if let Some(mut w) = guard.take() {
        w.stop();
    }
    Ok(true)
}

// ---------------------------------------------------------------------------
// cache admin FFI
// ---------------------------------------------------------------------------

fn lua_clear_cache(_lua: &Lua, kind: Option<String>) -> LuaResult<bool> {
    let kind = kind.unwrap_or_else(|| "all".to_string());
    match kind.as_str() {
        "line" | "lines" | "content" => cache::clear_line_indexes().map_err(core_err)?,
        "metadata" => {
            // clear via a single call pattern: clear_all is "everything";
            // for metadata only we inline remove-and-clear via reopen
            let guard = cache::shared_metadata();
            let g = guard.read().map_err(core_err)?;
            if let Some(db) = g.as_ref() {
                db.clear().map_err(core_err)?;
            }
        }
        "all" | _ => cache::clear_all().map_err(core_err)?,
    }
    Ok(true)
}

fn lua_metadata_info(lua: &Lua, _: ()) -> LuaResult<LuaValue> {
    let result = lua.create_table()?;
    result.set(
        "path",
        cache::metadata_path()
            .map_err(core_err)?
            .unwrap_or_default(),
    )?;
    result.set("count", cache::metadata_count().map_err(core_err)?)?;
    result.set("disk_size", cache::metadata_disk_size().map_err(core_err)?)?;
    Ok(LuaValue::Table(result))
}

fn lua_invalidate_path(_lua: &Lua, path: String) -> LuaResult<bool> {
    let p = std::path::Path::new(&path);
    cache::invalidate_line_index_for(p).map_err(core_err)?;
    let _ = cache::remove_metadata_entry(&path).map_err(core_err)?;
    Ok(true)
}

fn lua_extract_specialized(lua: &Lua, path: String) -> LuaResult<LuaValue> {
    let result = specialized::extract_specialized(&path).map_err(core_err)?;
    lua.to_value(&result)
}

fn lua_prefetch_spawn(_lua: &Lua, _: ()) -> LuaResult<bool> {
    prefetch::spawn().map_err(core_err)?;
    Ok(true)
}

fn lua_semantic_get(lua: &Lua, path: String) -> LuaResult<LuaValue> {
    match cache::get_semantic(&path).map_err(core_err)? {
        Some(record) => lua.to_value(&record),
        None => Ok(LuaValue::Nil),
    }
}

fn lua_semantic_upsert(_lua: &Lua, args: LuaTable) -> LuaResult<bool> {
    let path: String = args.get("path")?;
    let revision: String = args.get("revision")?;
    let chunks_tbl: LuaTable = args.get("chunks")?;

    let mut chunks = Vec::new();
    for pair in chunks_tbl.sequence_values::<LuaTable>() {
        let t = pair?;
        chunks.push(ffr_core::db::SemanticChunk {
            start_line: t.get("start_line").unwrap_or(0u64),
            end_line: t.get("end_line").unwrap_or(0u64),
            kind: t.get("kind").unwrap_or_default(),
            name: t.get("name").ok(),
        });
    }
    let record = ffr_core::db::SemanticRecord { revision, chunks };
    cache::upsert_semantic(&path, &record).map_err(core_err)?;
    Ok(true)
}

fn lua_semantic_remove(_lua: &Lua, path: String) -> LuaResult<bool> {
    cache::remove_semantic(&path).map_err(core_err)
}

fn lua_prefetch_hint(_lua: &Lua, args: LuaTable) -> LuaResult<bool> {
    let path: String = args.get("path")?;
    let chunk_id: u64 = args.get("chunk_id")?;
    let chunk_bytes: usize = args.get("chunk_bytes")?;
    let count: Option<u64> = args.get("count").ok();
    match count {
        Some(n) if n > 0 => {
            prefetch::hint_range(&path, chunk_id, n, chunk_bytes).map_err(core_err)?
        }
        _ => prefetch::hint(&path, chunk_id, chunk_bytes).map_err(core_err)?,
    }
    Ok(true)
}
