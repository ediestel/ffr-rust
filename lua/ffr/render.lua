local M = {}

local buffer = require("ffr.buffer")

local HEADER_NS = vim.api.nvim_create_namespace('ffr.preview_header')

---@param bufnr integer
---@param session table
---@param result table|nil  chunk result (for chunk/preview renders)
local function set_preview_header(bufnr, session, result)
  if not vim.api.nvim_buf_is_valid(bufnr) then return end

  local c = session.classification or {}
  local parts = { 'ffr:' .. tostring(session.mode or '?') }
  if c.kind then table.insert(parts, c.kind) end
  if c.encoding then table.insert(parts, c.encoding) end
  if result and result.chunk_id ~= nil then
    local label = string.format('chunk %d', result.chunk_id)
    if result.eof then label = label .. ' [eof]' end
    table.insert(parts, label)
  end
  if c.likely_filetype and c.likely_filetype ~= '' then
    table.insert(parts, c.likely_filetype)
  end
  local text = '-- ' .. table.concat(parts, ' · ')

  -- Wipe previous header marks, then re-set.
  vim.api.nvim_buf_clear_namespace(bufnr, HEADER_NS, 0, -1)
  vim.api.nvim_buf_set_extmark(bufnr, HEADER_NS, 0, 0, {
    virt_lines = { { { text, 'FFRPreviewHeader' } } },
    virt_lines_above = true,
  })
end

---@param bufnr integer
---@param filetype string|nil
local function apply_treesitter(bufnr, filetype)
  if not filetype or filetype == '' then return end

  local ok_cfg, config_mod = pcall(require, 'ffr.config')
  if ok_cfg then
    local cfg = config_mod.get()
    if cfg.preview and cfg.preview.syntax_highlight == false then
      return
    end
  end

  if vim.treesitter and vim.treesitter.start then
    local ok = pcall(vim.treesitter.start, bufnr, filetype)
    if not ok then
      -- Fall back to Vim syntax — nvim_set_option_value('filetype',...) already did this
    end
  end
end

---@param bufnr integer
local function maybe_set_linenumbers(bufnr)
  local ok_cfg, config_mod = pcall(require, 'ffr.config')
  if not ok_cfg then return end
  local cfg = config_mod.get()
  if cfg.preview and cfg.preview.line_numbers == false then return end
  for _, winid in ipairs(vim.api.nvim_list_wins()) do
    if vim.api.nvim_win_get_buf(winid) == bufnr then
      pcall(vim.api.nvim_set_option_value, 'number', true, { win = winid })
    end
  end
end

---@param text string
---@return string[]
local function split_lines(text)
  if text == "" then
    return { "" }
  end
  return vim.split(text or "", "\n", { plain = true })
end

---@param session table
---@param result table
---@return boolean ok, string? err
function M.render_full(session, result)
  if not session or not session.bufnr then
    return false, "invalid session"
  end

  local lines = result.lines
  if type(lines) ~= "table" then
    if type(result.data) == "string" then
      lines = split_lines(result.data)
    else
      return false, "missing full render content"
    end
  end

  local ft = session.classification and session.classification.likely_filetype or nil
  buffer.set_lines(session.bufnr, lines)
  buffer.set_name(session.bufnr, session.path)
  buffer.set_filetype(session.bufnr, ft)
  apply_treesitter(session.bufnr, ft)
  maybe_set_linenumbers(session.bufnr)
  set_preview_header(session.bufnr, session, nil)
  buffer.set_modifiable(session.bufnr, false)

  return true, nil
end

---@param session table
---@param result table
---@return boolean ok, string? err
function M.render_chunk(session, result)
  if not session or not session.bufnr then
    return false, "invalid session"
  end

  local text = result.text
  if type(text) ~= "string" then
    return false, "missing chunk text"
  end

  local ft = session.classification and session.classification.likely_filetype or nil
  buffer.set_lines(session.bufnr, split_lines(text))
  buffer.set_name(session.bufnr, session.path)
  buffer.set_filetype(session.bufnr, ft)
  apply_treesitter(session.bufnr, ft)
  maybe_set_linenumbers(session.bufnr)
  set_preview_header(session.bufnr, session, result)
  buffer.set_modifiable(session.bufnr, false)

  return true, nil
end

---@param session table
---@param message string
---@return boolean ok
function M.render_rejection(session, message)
  if not session or not session.bufnr then
    return false
  end

  buffer.set_lines(session.bufnr, {
    "ffr: open rejected",
    "",
    message or "unknown reason",
  })
  buffer.set_name(session.bufnr, session.path or "[ffr]")
  buffer.set_modifiable(session.bufnr, false)

  return true
end

---@param session table
---@return boolean ok, string? err
function M.render_info(session)
  if not session or not session.bufnr then
    return false, "invalid session"
  end

  local lines = {
    "ffr session info",
    "",
    "path: " .. tostring(session.path),
    "mode: " .. tostring(session.mode),
    "session_id: " .. tostring(session.id),
    "current_chunk: " .. tostring(session.current_chunk),
    "eof: " .. tostring(session.eof),
    "line_index_ready: " .. tostring(session.line_index_ready),
  }

  buffer.set_lines(session.bufnr, lines)
  buffer.set_modifiable(session.bufnr, false)

  return true, nil
end

---@param session table
---@param result table
---@return boolean ok, string? err
function M.render(session, result)
  if session.mode == "full" then
    return M.render_full(session, result)
  end
  return M.render_chunk(session, result)
end

return M