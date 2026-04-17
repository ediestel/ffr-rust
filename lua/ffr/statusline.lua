--- Statusline helper.
---
--- Usage from your statusline config (lualine, heirline, or raw):
---   local ffr_status = require('ffr.statusline').get(0)
---
--- Returns an empty string when the current buffer is not an ffr buffer.

local M = {}

local function human_size(bytes)
  if not bytes or bytes == 0 then return '' end
  local units = { 'B', 'KB', 'MB', 'GB' }
  local i = 1
  while bytes >= 1024 and i < #units do
    bytes = bytes / 1024
    i = i + 1
  end
  return string.format('%.1f%s', bytes, units[i])
end

---@param bufnr? integer
---@return string
function M.get(bufnr)
  bufnr = bufnr or vim.api.nvim_get_current_buf()
  if not vim.api.nvim_buf_is_valid(bufnr) then return '' end

  local mode = vim.b[bufnr].ffr_mode
  if not mode or mode == '' then return '' end

  local session_mod = require('ffr.session')
  local session = session_mod.get_by_bufnr and session_mod.get_by_bufnr(bufnr) or nil

  local parts = { 'ffr:' .. tostring(mode) }
  if session then
    if session.current_chunk ~= nil then
      local total = session.classification and session.classification.estimated_lines or nil
      table.insert(parts, string.format('chunk %d', session.current_chunk))
      if session.eof then
        table.insert(parts, '[eof]')
      end
      if total then
        table.insert(parts, string.format('~%d lines', total))
      end
    end
    if session.classification then
      local c = session.classification
      if c.encoding then table.insert(parts, c.encoding) end
      if c.likely_filetype and c.likely_filetype ~= '' then
        table.insert(parts, c.likely_filetype)
      end
    end
  end

  local size = vim.b[bufnr].ffr_size
  if size then table.insert(parts, human_size(size)) end

  return table.concat(parts, ' · ')
end

return M
