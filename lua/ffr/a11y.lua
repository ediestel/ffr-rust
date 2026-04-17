--- Accessibility helpers. When `config.accessibility.enabled = true`, these
--- announce chunk changes and policy decisions via `vim.notify`.

local M = {}

local function enabled()
  local ok, cfg = pcall(require, 'ffr.config')
  if not ok then return false end
  local v = cfg.get()
  return v.accessibility and v.accessibility.enabled == true
end

function M.announce_chunk(current, total)
  if not enabled() then return end
  local cfg = require('ffr.config').get()
  if not cfg.accessibility.announce_chunks then return end
  vim.notify(
    string.format('ffr: chunk %d of %s', current, total or '?'),
    vim.log.levels.INFO,
    { title = 'ffr' }
  )
end

function M.announce_policy(decision, reason)
  if not enabled() then return end
  local msg = 'ffr: ' .. tostring(decision)
  if reason and reason ~= '' then msg = msg .. ' — ' .. reason end
  vim.notify(msg, vim.log.levels.INFO, { title = 'ffr' })
end

function M.announce_rejection(reason)
  if not enabled() then return end
  vim.notify('ffr: rejected — ' .. tostring(reason), vim.log.levels.WARN, { title = 'ffr' })
end

return M
