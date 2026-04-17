--- Render archive listing into a buffer.

local buffer = require('ffr.buffer')

local M = {}

local function human_size(bytes)
  if not bytes then return '?' end
  local units = { 'B', 'KB', 'MB', 'GB' }
  local i = 1
  while bytes >= 1024 and i < #units do
    bytes = bytes / 1024
    i = i + 1
  end
  return string.format('%6.1f %s', bytes, units[i])
end

---@param path string
---@param content table
---@return integer bufnr
function M.render(path, content)
  local bufnr = buffer.create_buffer(true, true)
  buffer.set_ffr_vars(bufnr, path, 'specialized:archive', nil)

  local lines = { content.summary or ('[archive] ' .. path), '' }
  if content.metadata and #content.metadata > 0 then
    for _, kv in ipairs(content.metadata) do
      table.insert(lines, string.format('%s = %s', kv[1], kv[2]))
    end
    table.insert(lines, '')
  end

  if content.entries and #content.entries > 0 then
    table.insert(lines, '# entries')
    for _, entry in ipairs(content.entries) do
      local marker = entry.is_dir and 'd' or '-'
      table.insert(lines, string.format('%s %s  %s', marker, human_size(entry.size), entry.name))
    end
  end

  buffer.set_lines(bufnr, lines)
  buffer.set_name(bufnr, path)
  vim.api.nvim_buf_set_option(bufnr, 'modifiable', false)
  buffer.ensure_visible(bufnr)
  return bufnr
end

return M
