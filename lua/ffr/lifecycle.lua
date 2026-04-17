--- Lifecycle hooks: init + shutdown. Ensures LMDB + watcher are torn down
--- cleanly on VimLeavePre.

local M = {}

local init_hooks = {}
local shutdown_hooks = {}
local state = { initialized = false, shutdown_complete = false }

---@param callback fun()
function M.on_init(callback)
  table.insert(init_hooks, callback)
end

---@param callback fun()
function M.on_shutdown(callback)
  table.insert(shutdown_hooks, callback)
end

function M.init()
  if state.initialized then return end
  state.initialized = true

  for _, cb in ipairs(init_hooks) do
    local ok, err = pcall(cb)
    if not ok then
      vim.notify('ffr.nvim: init hook error: ' .. tostring(err), vim.log.levels.WARN)
    end
  end

  vim.api.nvim_create_autocmd('VimLeavePre', {
    group = vim.api.nvim_create_augroup('ffr_lifecycle', { clear = true }),
    desc = 'ffr.nvim graceful shutdown',
    callback = function() M.shutdown() end,
  })
end

function M.shutdown()
  if state.shutdown_complete then return end
  state.shutdown_complete = true

  for _, cb in ipairs(shutdown_hooks) do
    local ok, err = pcall(cb)
    if not ok then
      vim.notify('ffr.nvim: shutdown hook error: ' .. tostring(err), vim.log.levels.WARN)
    end
  end

  local ok, client = pcall(require, 'ffr.client')
  if ok and client.stop then
    pcall(client.stop)
  end
end

---@return boolean
function M.is_initialized()
  return state.initialized
end

return M
