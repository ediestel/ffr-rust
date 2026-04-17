--- Render extracted PDF text into a buffer.

local buffer = require('ffr.buffer')

local M = {}

---@param path string
---@param content table  SpecializedContent result from Rust
---@return integer bufnr
function M.render(path, content)
  local bufnr = buffer.create_buffer(true, true)
  buffer.set_ffr_vars(bufnr, path, 'specialized:pdf', nil)

  local header = { content.summary or ('[pdf] ' .. path), '' }
  if content.metadata and #content.metadata > 0 then
    table.insert(header, '# metadata')
    for _, kv in ipairs(content.metadata) do
      table.insert(header, string.format('%s = %s', kv[1], kv[2]))
    end
    table.insert(header, '')
    table.insert(header, '# text')
    table.insert(header, '')
  end

  local lines = {}
  for _, h in ipairs(header) do table.insert(lines, h) end
  if content.text and content.text ~= '' then
    for line in (content.text .. '\n'):gmatch('(.-)\n') do
      table.insert(lines, line)
    end
  else
    table.insert(lines, '(no extractable text)')
  end

  buffer.set_lines(bufnr, lines)
  buffer.set_name(bufnr, path)
  vim.api.nvim_buf_set_option(bufnr, 'modifiable', false)
  buffer.ensure_visible(bufnr)
  return bufnr
end

return M
