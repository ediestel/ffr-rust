local M = {}

local ffr = require('ffr')

---@param name string
---@param fn fun(args: table)
---@param opts? table
local function cmd(name, fn, opts)
  opts = opts or {}
  vim.api.nvim_create_user_command(name, function(args) fn(args) end, opts)
end

function M.create_user_commands()
  cmd('FFR', function(args) M.cmd_open(args) end, { nargs = 1, complete = 'file' })
  cmd('FFRPreview', function(args) M.cmd_preview(args) end, { nargs = 1, complete = 'file' })
  cmd('FFRChunkNext', function() M.cmd_next_chunk() end, { nargs = 0 })
  cmd('FFRChunkPrev', function() M.cmd_prev_chunk() end, { nargs = 0 })
  cmd('FFRReload', function() M.cmd_reload() end, { nargs = 0 })
  cmd('FFRInfo', function() M.cmd_info() end, { nargs = 0 })

  -- New commands (Phase B2)
  cmd('FFRHealth', function() M.cmd_health() end, { nargs = 0 })
  cmd('FFRDebug', function(args) M.cmd_debug(args) end, {
    nargs = '?',
    complete = function() return { 'on', 'off', 'toggle' } end,
  })
  cmd('FFRClearCache', function(args) M.cmd_clear_cache(args) end, {
    nargs = '?',
    complete = function() return { 'all', 'metadata', 'content' } end,
  })
  cmd('FFROpenLog', function() M.cmd_open_log() end, { nargs = 0 })
  cmd('FFRWatchStatus', function() M.cmd_watch_status() end, { nargs = 0 })
  cmd('FFRInvalidate', function(args) M.cmd_invalidate(args) end, { nargs = 1, complete = 'file' })

  -- Semantic navigation (Phase C3)
  cmd('FFRChunkNextSemantic', function() M.cmd_semantic_next() end, { nargs = 0 })
  cmd('FFRChunkPrevSemantic', function() M.cmd_semantic_prev() end, { nargs = 0 })
end

-- Existing commands --------------------------------------------------------

function M.cmd_open(args)
  local _, err = ffr.open(args.args, {})
  if err then vim.notify('FFR: ' .. err, vim.log.levels.ERROR) end
end

function M.cmd_preview(args)
  local _, err = ffr.preview(args.args, {})
  if err then vim.notify('FFRPreview: ' .. err, vim.log.levels.ERROR) end
end

function M.cmd_next_chunk()
  local ok, err = ffr.next_chunk()
  if not ok then vim.notify('FFRChunkNext: ' .. tostring(err), vim.log.levels.ERROR) end
end

function M.cmd_prev_chunk()
  local ok, err = ffr.prev_chunk()
  if not ok then vim.notify('FFRChunkPrev: ' .. tostring(err), vim.log.levels.ERROR) end
end

function M.cmd_reload()
  local ok, err = ffr.reload()
  if not ok then vim.notify('FFRReload: ' .. tostring(err), vim.log.levels.ERROR) end
end

function M.cmd_info()
  local info, err = ffr.info()
  if err then
    vim.notify('FFRInfo: ' .. tostring(err), vim.log.levels.ERROR)
    return
  end
  vim.notify(vim.inspect(info), vim.log.levels.INFO)
end

-- New commands -------------------------------------------------------------

function M.cmd_health()
  local report = require('ffr.health').run()
  vim.notify(vim.inspect(report), vim.log.levels.INFO, { title = 'FFRHealth' })
end

function M.cmd_debug(args)
  local state = require('ffr').debug_state or { enabled = false }
  local input = (args.args or ''):gsub('^%s+', ''):gsub('%s+$', '')
  local new
  if input == 'on' then
    new = true
  elseif input == 'off' then
    new = false
  else
    new = not state.enabled
  end
  state.enabled = new
  require('ffr').debug_state = state
  vim.notify('FFR debug: ' .. (new and 'on' or 'off'), vim.log.levels.INFO)
end

function M.cmd_clear_cache(args)
  local kind = (args.args or 'all'):gsub('^%s+', ''):gsub('%s+$', '')
  if kind == '' then kind = 'all' end
  local mod, err = require('ffr.rust').get()
  if not mod then
    vim.notify('FFRClearCache: backend unavailable: ' .. tostring(err), vim.log.levels.ERROR)
    return
  end
  local ok, result = pcall(mod.clear_cache, kind)
  if ok and result then
    vim.notify('FFR cache cleared: ' .. kind, vim.log.levels.INFO)
  else
    vim.notify('FFRClearCache failed: ' .. tostring(result), vim.log.levels.ERROR)
  end
end

function M.cmd_open_log()
  local cfg = require('ffr.config').get()
  local path = cfg.logging and cfg.logging.file
  if not path or path == '' then
    vim.notify('FFROpenLog: logging disabled or file unset', vim.log.levels.WARN)
    return
  end
  if vim.fn.filereadable(path) ~= 1 then
    vim.notify('FFROpenLog: log file not found: ' .. path, vim.log.levels.WARN)
    return
  end
  vim.cmd.tabnew(vim.fn.fnameescape(path))
end

function M.cmd_watch_status()
  local mod, err = require('ffr.rust').get()
  if not mod then
    vim.notify('FFRWatchStatus: backend unavailable: ' .. tostring(err), vim.log.levels.ERROR)
    return
  end
  local ok, status = pcall(mod.watcher_status)
  if not ok then
    vim.notify('FFRWatchStatus: ' .. tostring(status), vim.log.levels.ERROR)
    return
  end
  vim.notify(vim.inspect(status), vim.log.levels.INFO, { title = 'FFR watcher' })
end

local function jump_semantic(direction)
  local bufnr = vim.api.nvim_get_current_buf()
  local line = vim.api.nvim_win_get_cursor(0)[1]
  local target = require('ffr.semantic').find_neighbor(bufnr, line, direction)
  if not target then
    vim.notify('FFR: no ' .. direction .. ' semantic chunk', vim.log.levels.INFO)
    return
  end
  vim.api.nvim_win_set_cursor(0, { target.start_line, 0 })
  local label = target.name and (target.kind .. ' ' .. target.name) or target.kind
  vim.notify('FFR: ' .. label, vim.log.levels.INFO)
end

function M.cmd_semantic_next() jump_semantic('next') end
function M.cmd_semantic_prev() jump_semantic('prev') end

function M.cmd_invalidate(args)
  local mod, err = require('ffr.rust').get()
  if not mod then
    vim.notify('FFRInvalidate: backend unavailable: ' .. tostring(err), vim.log.levels.ERROR)
    return
  end
  local path = args.args
  local ok, result = pcall(mod.invalidate_path, vim.fn.fnamemodify(path, ':p'))
  if ok and result then
    vim.notify('FFR invalidated: ' .. path, vim.log.levels.INFO)
  else
    vim.notify('FFRInvalidate failed: ' .. tostring(result), vim.log.levels.ERROR)
  end
end

return M
