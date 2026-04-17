--- ffr health checks. Integrates with :checkhealth ffr.
---
--- Reports on:
---   - Native backend (ffr-nvim CDylib) loadable
---   - Tracing log file path
---   - LMDB metadata store (path, entry count, disk size)
---   - File watcher status (running, watched paths count)
---   - Classify roundtrip against a known good file
---   - MCP binary presence on PATH

local M = {}

local function start_rust()
  local ok, rust = pcall(require, 'ffr.rust')
  if not ok then
    return nil, 'failed to require ffr.rust: ' .. tostring(rust)
  end
  local mod, err = rust.get()
  return mod, err
end

--- Collect structured health information.
--- @param opts? { test_path?: string }
--- @return table report
function M.run(opts)
  opts = opts or {}

  local report = {
    ok = true,
    backend = { available = false, error = nil },
    metadata = { path = nil, count = 0, disk_size = 0, error = nil },
    watcher = { running = false, watched_count = 0, error = nil },
    classify_roundtrip = { ok = false, path = nil, error = nil },
    mcp_binary = { available = false, path = nil, error = nil },
  }

  -- Backend
  local mod, err = start_rust()
  if not mod then
    report.backend.error = err
    report.ok = false
  else
    report.backend.available = true
  end

  -- Metadata DB
  if mod and mod.metadata_info then
    local ok_info, info = pcall(mod.metadata_info)
    if ok_info and type(info) == 'table' then
      report.metadata.path = info.path
      report.metadata.count = info.count or 0
      report.metadata.disk_size = info.disk_size or 0
    else
      report.metadata.error = tostring(info)
    end
  end

  -- Watcher
  if mod and mod.watcher_status then
    local ok_w, status = pcall(mod.watcher_status)
    if ok_w and type(status) == 'table' then
      report.watcher.running = status.running or false
      report.watcher.watched_count = status.watched and #status.watched or 0
    else
      report.watcher.error = tostring(status)
    end
  end

  -- Classify roundtrip
  local test_path = opts.test_path
  if not test_path then
    local src = debug.getinfo(1, 'S').source
    if src and src:sub(1, 1) == '@' then
      test_path = src:sub(2)
    end
  end

  if mod and test_path then
    report.classify_roundtrip.path = test_path
    local cfg_ok, cfg = pcall(function() return require('ffr.config').get() end)
    if cfg_ok and type(cfg) == 'table' then
      local ok_c, result = pcall(mod.classify_path, {
        path = test_path,
        sniff_bytes = cfg.binary_sniff_bytes,
        full_open_max_bytes = cfg.full_open_max_bytes,
        minified_line_length_threshold = cfg.minified_line_length_threshold,
      })
      if ok_c and type(result) == 'table' then
        report.classify_roundtrip.ok = true
      else
        report.classify_roundtrip.error = tostring(result)
        report.ok = false
      end
    end
  end

  -- MCP binary
  local exepath = vim.fn.exepath('ffr-mcp')
  if exepath and exepath ~= '' then
    report.mcp_binary.available = true
    report.mcp_binary.path = exepath
  end

  return report
end

--- Pretty-print for :checkhealth ffr. Uses vim.health if available.
function M.check()
  local health = vim.health or { start = vim.fn['health#report_start'], ok = vim.fn['health#report_ok'], warn = vim.fn['health#report_warn'], error = vim.fn['health#report_error'], info = vim.fn['health#report_info'] }
  local report = M.run()

  health.start('ffr: backend')
  if report.backend.available then
    health.ok('native backend loaded')
  else
    health.error('native backend unavailable: ' .. (report.backend.error or 'unknown'))
  end

  health.start('ffr: metadata store (LMDB)')
  if report.metadata.path and report.metadata.path ~= '' then
    health.ok(string.format('path: %s', report.metadata.path))
    health.info(string.format('entries: %d · disk: %d bytes', report.metadata.count, report.metadata.disk_size))
  else
    health.warn('metadata store not initialized (configure() not yet called?)')
  end

  health.start('ffr: watcher')
  if report.watcher.running then
    health.ok(string.format('running; %d file(s) watched', report.watcher.watched_count))
  else
    health.info('not running (enable via config.watcher.enabled)')
  end

  health.start('ffr: classify roundtrip')
  if report.classify_roundtrip.ok then
    health.ok('classify_path works: ' .. (report.classify_roundtrip.path or '?'))
  elseif report.classify_roundtrip.error then
    health.error('classify_path failed: ' .. report.classify_roundtrip.error)
  else
    health.info('skipped (no test path)')
  end

  health.start('ffr: MCP binary')
  if report.mcp_binary.available then
    health.ok('ffr-mcp on PATH: ' .. report.mcp_binary.path)
  else
    health.warn("ffr-mcp not on PATH (run 'make build' to build)")
  end
end

return M
