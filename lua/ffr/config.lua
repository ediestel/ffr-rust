local M = {}

-- Annotations for FFRConfig and nested tables live in lua/ffr/types.lua so
-- consumers get EmmyLua types without transitively loading this module.
require('ffr.types')

local validation = require('ffr.config_validation')

---@type FFRConfig
M.defaults = {
  backend_cmd = { 'ffr-mcp' },
  request_timeout_ms = 5000,
  preview_max_bytes = 256 * 1024,
  full_open_max_bytes = 2 * 1024 * 1024,
  chunk_bytes = 64 * 1024,
  max_line_window = 2000,
  binary_sniff_bytes = 4096,
  minified_line_length_threshold = 1000,
  enable_persistent_metadata_cache = true,
  metadata_cache_path = nil,

  logging = {
    enabled = true,
    level = 'info',
    file = nil,
    max_files = 3,
  },
  watcher = {
    enabled = true,
    debounce_ms = 250,
  },
  encodings = {
    fallback_order = { 'utf-8', 'utf-16le', 'utf-16be', 'windows-1252', 'latin1' },
  },
  preview = {
    syntax_highlight = true,
    line_numbers = true,
    max_preview_chunks = 4,
  },
  chunk = {
    prefetch_count = 1,
    keymaps = {
      enabled = true,
      next = ']c',
      prev = '[c',
    },
  },
  specialized = {
    pdf = { enabled = true, max_pages = 50 },
    image = { enabled = true, show_metadata = true },
    archive = { enabled = true, max_entries = 500 },
  },
  accessibility = {
    enabled = false,
    announce_chunks = false,
  },
  highlights = {
    chunk_boundary = nil,
    preview_header = nil,
    rejected_banner = nil,
    binary_hex = nil,
  },
  hooks = {
    on_classify = nil,
    on_open = nil,
    on_chunk_load = nil,
  },
}

---@type FFRConfig
M.values = nil

local function resolve_metadata_cache_path(values)
  if values.metadata_cache_path == nil and values.enable_persistent_metadata_cache then
    local data_dir = vim.fn.stdpath('data')
    local legacy_json = data_dir .. '/ffr/metadata_cache.json'
    local lmdb_dir = data_dir .. '/ffr/metadata-db'
    if vim.fn.filereadable(legacy_json) == 1 then
      values.metadata_cache_path = legacy_json
    else
      values.metadata_cache_path = lmdb_dir
    end
  end
end

local function resolve_logging_path(values)
  if values.logging and values.logging.enabled and values.logging.file == nil then
    local data_dir = vim.fn.stdpath('data')
    values.logging.file = data_dir .. '/ffr/ffr.log'
  end
end

---@param opts? table
---@return FFRConfig
function M.setup(opts)
  opts = opts or {}

  -- Migrate any deprecated flat keys (emits warnings), then merge.
  opts = validation.migrate_deprecated(opts)

  M.values = vim.tbl_deep_extend('force', {}, M.defaults, opts)

  resolve_metadata_cache_path(M.values)
  resolve_logging_path(M.values)

  validation.validate(M.values)

  return M.values
end

---@return FFRConfig
function M.get()
  if not M.values then
    return M.setup()
  end
  return M.values
end

function M.reset()
  M.values = nil
end

return M
