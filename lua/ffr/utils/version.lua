local M = {}

local is_windows = (package.config:sub(1, 1) == '\\')

local function shell_quote(s)
  if is_windows then
    return '"' .. s:gsub('"', '\\"') .. '"'
  end
  return "'" .. s:gsub("'", "'\\''") .. "'"
end

local function git(repo_root, ...)
  local parts = { 'git', '-C', shell_quote(repo_root) }
  for i = 1, select('#', ...) do
    parts[#parts + 1] = shell_quote(select(i, ...))
  end

  local redirect = is_windows and ' 2>NUL' or ' 2>/dev/null'
  local handle = io.popen(table.concat(parts, ' ') .. redirect)
  if not handle then return nil end

  local output = handle:read('*a')
  handle:close()

  if not output or output:match('^%s*$') then return nil end
  return output:gsub('%s+$', '')
end

function M.read_base_version(repo_root)
  local cargo_path = repo_root .. '/crates/ffr-core/Cargo.toml'
  local f = io.open(cargo_path, 'r')
  if not f then return nil end

  for line in f:lines() do
    local ver = line:match('^version%s*=%s*"([^"]+)"')
    if ver then
      f:close()
      return ver
    end
  end

  f:close()
  return nil
end

local function bump_patch(version)
  local major, minor, patch = version:match('^(%d+)%.(%d+)%.(%d+)')
  if not major then return nil end
  return string.format('%s.%s.%d', major, minor, tonumber(patch) + 1)
end

---@class FFRVersionInfo
---@field version string semver version
---@field release_tag string GitHub release tag for download URLs
---@field is_release boolean true for tagged stable releases

function M.resolve(repo_root)
  local tag = git(repo_root, 'describe', '--exact-match', '--tags', '--match', 'v*', 'HEAD')

  if tag and tag:match('^v%d') then
    return {
      version = tag:sub(2),
      release_tag = tag,
      is_release = true,
    }
  end

  local short_sha = git(repo_root, 'rev-parse', '--short', 'HEAD')
  if not short_sha then return nil, 'Failed to determine git SHA' end

  local base_version = M.read_base_version(repo_root)
  if not base_version then return nil, 'Could not read base version from crates/ffr-core/Cargo.toml' end

  local next_version = bump_patch(base_version)
  if not next_version then return nil, 'Could not parse base version: ' .. base_version end

  local branch = git(repo_root, 'symbolic-ref', '--short', 'HEAD')

  local prerelease_label
  if not branch or branch == 'main' then
    prerelease_label = 'nightly'
  else
    prerelease_label = 'dev'
  end

  local version = string.format('%s-%s.%s', next_version, prerelease_label, short_sha)
  return {
    version = version,
    release_tag = version,
    is_release = false,
  }
end

return M
