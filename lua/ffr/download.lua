--- Download prebuilt ffr-nvim native binary. Mirrors fff.nvim's download flow.
---
--- On first plugin load: check whether a local target/release build exists.
--- If not, and the user hasn't disabled auto-download, fetch the pre-compiled
--- shared library for the current platform from the GitHub releases of the
--- repository configured below.
---
--- NOTE: until CI publishes releases (Phase D2), the fallback local build via
--- `make build` is the operative path. `download.lua` is otherwise ready.

local M = {}

local system = require('ffr.utils.system')
local fs_utils = require('ffr.utils.fs')

-- TODO(Phase D2): swap this once the repo is published.
local GITHUB_REPO = 'eckhartd/ffr.nvim'

local function get_plugin_dir()
  local src = debug.getinfo(1, 'S').source
  if not src or src:sub(1, 1) ~= '@' then return nil end
  local this_file = src:sub(2)
  -- this_file = .../lua/ffr/download.lua -> plugin root is two levels up
  return vim.fn.fnamemodify(this_file, ':h:h:h')
end

local function binary_dir()
  local plugin_dir = get_plugin_dir()
  if not plugin_dir then return nil end
  return plugin_dir .. '/target/release'
end

local function binary_path()
  local dir = binary_dir()
  if not dir then return nil end
  return dir .. '/libffr_nvim.' .. system.get_lib_extension()
end

function M.get_binary_path()
  return binary_path()
end

function M.binary_exists()
  local p = binary_path()
  if not p then return false end
  return fs_utils.is_file(p)
end

local function detect_triple()
  return system.get_triple()
end

local function release_url(tag, filename)
  return string.format(
    'https://github.com/%s/releases/download/%s/%s',
    GITHUB_REPO,
    tag,
    filename
  )
end

local function curl_download(url, output_path, callback)
  local dir = vim.fn.fnamemodify(output_path, ':h')
  fs_utils.mkdir_recursive(dir, function(ok, err)
    if not ok then return callback(false, err) end

    local args = {
      'curl',
      '--fail',
      '--location',
      '--silent',
      '--show-error',
      '--output',
      output_path,
      url,
    }
    vim.system(args, {}, function(result)
      if result.code ~= 0 then
        callback(false, 'curl failed: ' .. (result.stderr or 'unknown error'))
      else
        callback(true, nil)
      end
    end)
  end)
end

--- Attempt auto-download. Falls through to `make build` hint on failure.
---@param tag string
---@param callback fun(ok: boolean, err: string|nil)
function M.download(tag, callback)
  local triple = detect_triple()
  local ext = system.get_lib_extension()
  local filename = string.format('libffr_nvim-%s.%s', triple, ext)
  local url = release_url(tag, filename)
  local dest = binary_path()
  if not dest then
    return callback(false, 'cannot resolve binary path')
  end
  curl_download(url, dest, callback)
end

--- Synchronous fallback: build from source.
---@return boolean, string|nil
function M.build_from_source()
  local plugin_dir = get_plugin_dir()
  if not plugin_dir then return false, 'cannot locate plugin dir' end

  local cmd = { 'cargo', 'build', '--release', '-p', 'ffr-nvim' }
  local result = vim.system(cmd, { cwd = plugin_dir }):wait()
  if result.code ~= 0 then
    return false, result.stderr or 'cargo build failed'
  end
  return true, nil
end

--- Ensure a native binary is present. Attempt download first (if enabled),
--- fall back to local build.
---@param opts? { auto_download?: boolean, tag?: string }
---@return boolean, string|nil
function M.ensure(opts)
  opts = opts or {}
  if M.binary_exists() then return true, nil end

  if opts.auto_download ~= false then
    local tag = opts.tag or 'latest'
    local done, err = false, nil
    M.download(tag, function(ok, derr)
      done, err = ok, derr
    end)
    -- curl_download is async; spin until callback fires
    vim.wait(60000, function() return done or err ~= nil end, 50)
    if done then return true, nil end
    vim.notify('ffr: download failed (' .. tostring(err) .. '), falling back to local build', vim.log.levels.INFO)
  end

  return M.build_from_source()
end

return M
