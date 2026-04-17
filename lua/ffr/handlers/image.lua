--- Render image metadata into a buffer.

local buffer = require('ffr.buffer')

local M = {}

---@param path string
---@param content table
---@return integer bufnr
function M.render(path, content)
  local bufnr = buffer.create_buffer(true, true)
  buffer.set_ffr_vars(bufnr, path, 'specialized:image', nil)

  local lines = { content.summary or ('[image] ' .. path), '' }
  if content.metadata and #content.metadata > 0 then
    table.insert(lines, '# metadata')
    for _, kv in ipairs(content.metadata) do
      table.insert(lines, string.format('%s = %s', kv[1], kv[2]))
    end
  end

  buffer.set_lines(bufnr, lines)
  buffer.set_name(bufnr, path)
  vim.api.nvim_buf_set_option(bufnr, 'modifiable', false)
  buffer.ensure_visible(bufnr)
  return bufnr
end

return M
