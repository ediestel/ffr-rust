--- Config schema validation + deprecation migration.
---
--- Keeps `config.lua` uncluttered. Migrations run *before* merging defaults
--- so the user gets a clean structured config, and deprecation warnings are
--- emitted once per unique key.

local M = {}

local warned = {}

local function warn_once(key, msg)
  if warned[key] then return end
  warned[key] = true
  vim.notify('ffr.nvim: ' .. msg, vim.log.levels.WARN)
end

--- Migrate deprecated flat keys into the new nested schema. Returns a copy
--- with migrations applied; does not mutate the input.
---@param opts table
---@return table
function M.migrate_deprecated(opts)
  local out = vim.deepcopy(opts or {})

  -- Legacy flat key: `preview_max_bytes` -> `preview.max_preview_bytes`
  -- (We keep `preview_max_bytes` as a live option on the root for now because
  -- it was always public; this migration anticipates the nested rename.)
  if out.preview_max_bytes ~= nil and type(out.preview) == 'table' and out.preview.max_preview_bytes == nil then
    -- harmless duplicate; no warning
    out.preview.max_preview_bytes = out.preview_max_bytes
  end

  -- Legacy flat key: `chunk_keymaps_enabled` -> `chunk.keymaps.enabled`
  if out.chunk_keymaps_enabled ~= nil then
    warn_once(
      'chunk_keymaps_enabled',
      'option "chunk_keymaps_enabled" is deprecated; use "chunk = { keymaps = { enabled = ... } }"'
    )
    out.chunk = out.chunk or {}
    out.chunk.keymaps = out.chunk.keymaps or {}
    if out.chunk.keymaps.enabled == nil then
      out.chunk.keymaps.enabled = out.chunk_keymaps_enabled
    end
    out.chunk_keymaps_enabled = nil
  end

  -- Legacy flat key: `watcher_enabled` -> `watcher.enabled`
  if out.watcher_enabled ~= nil then
    warn_once(
      'watcher_enabled',
      'option "watcher_enabled" is deprecated; use "watcher = { enabled = ... }"'
    )
    out.watcher = out.watcher or {}
    if out.watcher.enabled == nil then
      out.watcher.enabled = out.watcher_enabled
    end
    out.watcher_enabled = nil
  end

  -- Legacy flat key: `log_level` -> `logging.level`
  if out.log_level ~= nil then
    warn_once(
      'log_level',
      'option "log_level" is deprecated; use "logging = { level = ... }"'
    )
    out.logging = out.logging or {}
    if out.logging.level == nil then
      out.logging.level = out.log_level
    end
    out.log_level = nil
  end

  return out
end

local VALID_LOG_LEVELS = { trace = true, debug = true, info = true, warn = true, error = true }

local function check_type(value, expected, key)
  if value == nil then return end
  if type(value) ~= expected then
    error(string.format('ffr.nvim: config.%s must be a %s, got %s', key, expected, type(value)))
  end
end

local function check_positive_int(value, key)
  if value == nil then return end
  if type(value) ~= 'number' or value < 0 or value ~= math.floor(value) then
    error(string.format('ffr.nvim: config.%s must be a non-negative integer, got %s', key, tostring(value)))
  end
end

--- Validate a merged config. Throws on structural errors.
---@param cfg table
function M.validate(cfg)
  check_positive_int(cfg.chunk_bytes, 'chunk_bytes')
  check_positive_int(cfg.full_open_max_bytes, 'full_open_max_bytes')
  check_positive_int(cfg.max_line_window, 'max_line_window')
  check_positive_int(cfg.binary_sniff_bytes, 'binary_sniff_bytes')
  check_positive_int(cfg.minified_line_length_threshold, 'minified_line_length_threshold')
  check_type(cfg.enable_persistent_metadata_cache, 'boolean', 'enable_persistent_metadata_cache')
  check_type(cfg.metadata_cache_path, 'string', 'metadata_cache_path')

  if cfg.logging then
    check_type(cfg.logging.enabled, 'boolean', 'logging.enabled')
    if cfg.logging.level and not VALID_LOG_LEVELS[cfg.logging.level] then
      error(string.format('ffr.nvim: config.logging.level must be one of trace/debug/info/warn/error, got %s', tostring(cfg.logging.level)))
    end
    check_type(cfg.logging.file, 'string', 'logging.file')
    check_positive_int(cfg.logging.max_files, 'logging.max_files')
  end

  if cfg.watcher then
    check_type(cfg.watcher.enabled, 'boolean', 'watcher.enabled')
    check_positive_int(cfg.watcher.debounce_ms, 'watcher.debounce_ms')
  end

  if cfg.encodings then
    if cfg.encodings.fallback_order ~= nil and type(cfg.encodings.fallback_order) ~= 'table' then
      error('ffr.nvim: config.encodings.fallback_order must be a list of strings')
    end
  end

  if cfg.preview then
    check_type(cfg.preview.syntax_highlight, 'boolean', 'preview.syntax_highlight')
    check_type(cfg.preview.line_numbers, 'boolean', 'preview.line_numbers')
    check_positive_int(cfg.preview.max_preview_chunks, 'preview.max_preview_chunks')
  end

  if cfg.chunk then
    check_positive_int(cfg.chunk.prefetch_count, 'chunk.prefetch_count')
    if cfg.chunk.keymaps then
      check_type(cfg.chunk.keymaps.enabled, 'boolean', 'chunk.keymaps.enabled')
      check_type(cfg.chunk.keymaps.next, 'string', 'chunk.keymaps.next')
      check_type(cfg.chunk.keymaps.prev, 'string', 'chunk.keymaps.prev')
    end
  end

  if cfg.specialized then
    for name, sub in pairs(cfg.specialized) do
      check_type(sub.enabled, 'boolean', 'specialized.' .. name .. '.enabled')
    end
  end

  if cfg.accessibility then
    check_type(cfg.accessibility.enabled, 'boolean', 'accessibility.enabled')
  end

  if cfg.hooks then
    for name, fn in pairs(cfg.hooks) do
      if fn ~= nil and type(fn) ~= 'function' then
        error(string.format('ffr.nvim: config.hooks.%s must be a function, got %s', name, type(fn)))
      end
    end
  end
end

--- Test helper to clear the deprecation-warning memoization.
function M._reset_warnings()
  warned = {}
end

return M
