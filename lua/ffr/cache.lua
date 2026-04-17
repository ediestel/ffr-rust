local M = {}

local config = require("ffr.config")

local metadata_cache = {}
local content_cache = {}

---@class FFRMetadataCacheEntry
---@field path string
---@field size integer
---@field mtime integer
---@field revision string
---@field classification table
---@field line_index_ready boolean
---@field line_count integer|nil

-- --- Metadata cache (persistent) ---

---@param path string
---@return FFRMetadataCacheEntry|nil
function M.get_metadata(path)
  return metadata_cache[path]
end

---@param path string
---@param entry FFRMetadataCacheEntry
function M.set_metadata(path, entry)
  metadata_cache[path] = entry
end

---@param path string
---@param stat_result table
---@return boolean valid
function M.is_metadata_valid(path, stat_result)
  local cached = metadata_cache[path]
  if not cached then
    return false
  end
  return cached.size == stat_result.size and cached.mtime == stat_result.mtime
end

---@param path string
function M.invalidate_metadata(path)
  metadata_cache[path] = nil
end

-- --- Content cache (ephemeral) ---

---@param path string
---@param chunk_id integer
---@return table|nil
function M.get_content_chunk(path, chunk_id)
  local by_path = content_cache[path]
  if not by_path then
    return nil
  end
  return by_path[chunk_id]
end

---@param path string
---@param chunk_id integer
---@param chunk table
function M.set_content_chunk(path, chunk_id, chunk)
  if not content_cache[path] then
    content_cache[path] = {}
  end
  content_cache[path][chunk_id] = chunk
end

---@param path string
function M.clear_content(path)
  content_cache[path] = nil
end

-- --- Persistence ---

function M.load_persistent_metadata()
  local cfg = config.get()
  if not cfg.enable_persistent_metadata_cache then
    return
  end

  local cache_path = cfg.metadata_cache_path
  if not cache_path or cache_path == "" then
    return
  end

  if vim.fn.filereadable(cache_path) ~= 1 then
    return
  end

  local lines = vim.fn.readfile(cache_path)
  if not lines or #lines == 0 then
    return
  end

  local raw = table.concat(lines, "\n")
  local ok, decoded = pcall(vim.json.decode, raw)
  if not ok or type(decoded) ~= "table" then
    return
  end

  metadata_cache = decoded
end

function M.save_persistent_metadata()
  local cfg = config.get()
  if not cfg.enable_persistent_metadata_cache then
    return
  end

  local cache_path = cfg.metadata_cache_path
  if not cache_path or cache_path == "" then
    return
  end

  local dir = vim.fn.fnamemodify(cache_path, ":h")
  if dir and dir ~= "" then
    vim.fn.mkdir(dir, "p")
  end

  local ok, encoded = pcall(vim.json.encode, metadata_cache)
  if not ok then
    return
  end
  vim.fn.writefile(vim.split(encoded, "\n", { plain = true }), cache_path)
end

return M
