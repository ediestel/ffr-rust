--- client.lua — sole bridge to the native backend.
--- In-process FFI via ffr-nvim CDylib (zero serialization, mirrors fff pattern).

local M = {}

local config = require("ffr.config")
local rust = require("ffr.rust")

local STATE = {
  started = false,
  configured = false,
}

-- ---------------------------------------------------------------------------
-- Startup
-- ---------------------------------------------------------------------------

---@return boolean ok, string? err
function M.start()
  if STATE.started then
    return true, nil
  end

  local mod, load_err = rust.get()
  if not mod then
    return false, load_err
  end

  STATE.started = true

  local configure_ok, configure_err = M.configure()
  if not configure_ok then
    STATE.started = false
    return false, configure_err
  end

  return true, nil
end

---@return boolean
function M.is_running()
  return STATE.started
end

---@return boolean ok
function M.stop()
  if STATE.started then
    local mod, _ = rust.get()
    if mod then
      pcall(mod.shutdown)
    end
    STATE.started = false
    STATE.configured = false
  end
  return true
end

---@return boolean ok, string? err
function M.restart()
  M.stop()
  return M.start()
end

-- ---------------------------------------------------------------------------
-- Configure
-- ---------------------------------------------------------------------------

---@return boolean ok, string? err
function M.configure()
  if STATE.configured then
    return true, nil
  end

  local mod, _ = rust.get()
  if not mod then
    return false, "ffr-nvim binary not loaded"
  end

  local cfg = config.get()
  local log_path = cfg.logging and cfg.logging.enabled and cfg.logging.file or nil
  local log_level = cfg.logging and cfg.logging.level or nil
  local ok, err = pcall(mod.configure, {
    metadata_cache_path = cfg.metadata_cache_path,
    log_path = log_path,
    log_level = log_level,
  })
  if not ok then
    return false, tostring(err)
  end

  STATE.configured = true
  return true, nil
end

-- ---------------------------------------------------------------------------
-- Public API — direct FFI calls, thresholds always from config
-- ---------------------------------------------------------------------------

---@param path string
---@return table|nil result, string|nil err
function M.stat_path(path)
  local ok, err = M.start()
  if not ok then
    return nil, err
  end

  local mod = rust.get()
  local call_ok, result = pcall(mod.stat_path, path)
  if not call_ok then
    return nil, tostring(result)
  end
  return result, nil
end

---@param path string
---@return table|nil result, string|nil err
function M.classify_path(path)
  local ok, err = M.start()
  if not ok then
    return nil, err
  end

  local mod = rust.get()
  local cfg = config.get()
  local call_ok, result = pcall(mod.classify_path, {
    path = path,
    sniff_bytes = cfg.binary_sniff_bytes,
    full_open_max_bytes = cfg.full_open_max_bytes,
    minified_line_length_threshold = cfg.minified_line_length_threshold,
  })
  if not call_ok then
    return nil, tostring(result)
  end
  return result, nil
end

---@param path string
---@param offset integer
---@param length integer
---@return table|nil result, string|nil err
function M.read_bytes(path, offset, length)
  local ok, err = M.start()
  if not ok then
    return nil, err
  end

  local mod = rust.get()
  local call_ok, result = pcall(mod.read_bytes, {
    path = path,
    offset = offset,
    length = length,
  })
  if not call_ok then
    return nil, tostring(result)
  end
  return result, nil
end

---@param path string
---@param start_line integer
---@param end_line integer
---@return table|nil result, string|nil err
function M.read_lines(path, start_line, end_line)
  local ok, err = M.start()
  if not ok then
    return nil, err
  end

  local mod = rust.get()
  local call_ok, result = pcall(mod.read_lines, {
    path = path,
    start_line = start_line,
    end_line = end_line,
  })
  if not call_ok then
    return nil, tostring(result)
  end
  return result, nil
end

---@param path string
---@return table|nil result, string|nil err
function M.build_line_index(path)
  local ok, err = M.start()
  if not ok then
    return nil, err
  end

  local mod = rust.get()
  local call_ok, result = pcall(mod.build_line_index, path)
  if not call_ok then
    return nil, tostring(result)
  end
  return result, nil
end

---@param path string
---@return table|nil result, string|nil err
function M.extract_specialized(path)
  local ok, err = M.start()
  if not ok then
    return nil, err
  end

  local mod = rust.get()
  if not mod or not mod.extract_specialized then
    return nil, "extract_specialized not available (rebuild ffr-nvim)"
  end
  local call_ok, result = pcall(mod.extract_specialized, path)
  if not call_ok then
    return nil, tostring(result)
  end
  return result, nil
end

---@param path string
---@param chunk_id integer
---@return table|nil result, string|nil err
function M.read_chunk(path, chunk_id)
  local ok, err = M.start()
  if not ok then
    return nil, err
  end

  local mod = rust.get()
  local cfg = config.get()
  local call_ok, result = pcall(mod.read_chunk, {
    path = path,
    chunk_id = chunk_id,
    chunk_bytes = cfg.chunk_bytes,
  })
  if not call_ok then
    return nil, tostring(result)
  end
  return result, nil
end

return M
