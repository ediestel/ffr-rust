--- Buffer-local keymaps auto-bound to ffr buffers.
---
--- Applied when a chunked/preview buffer is created and config.chunk.keymaps
--- has `enabled = true`. Users can override `next`/`prev` keys via config.

local M = {}

---@param bufnr integer
function M.bind_chunked(bufnr)
  local ok_cfg, config = pcall(require, 'ffr.config')
  if not ok_cfg then return end
  local cfg = config.get()
  local km = cfg.chunk and cfg.chunk.keymaps or nil
  if not km or km.enabled == false then return end

  local opts = { buffer = bufnr, silent = true, desc = 'ffr: next chunk' }
  pcall(vim.keymap.set, 'n', km.next or ']c', '<Cmd>FFRChunkNext<CR>', opts)
  pcall(vim.keymap.set, 'n', km.prev or '[c', '<Cmd>FFRChunkPrev<CR>',
    { buffer = bufnr, silent = true, desc = 'ffr: prev chunk' })

  pcall(vim.keymap.set, 'n', ']f', '<Cmd>FFRChunkNextSemantic<CR>',
    { buffer = bufnr, silent = true, desc = 'ffr: next semantic chunk' })
  pcall(vim.keymap.set, 'n', '[f', '<Cmd>FFRChunkPrevSemantic<CR>',
    { buffer = bufnr, silent = true, desc = 'ffr: prev semantic chunk' })
end

return M
