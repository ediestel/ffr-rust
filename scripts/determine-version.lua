-- Resolve the ffr version from git state. Invoked by CI
-- (.github/workflows/release.yml) and by release.sh.
--
-- Delegates to lua/ffr/utils/version.lua so the logic stays with the
-- plugin's other helpers.

local script_path = arg[0]
local script_dir = script_path:match('(.*[/\\])') or './'
local repo_root = script_dir .. '..'

package.path = repo_root .. '/lua/?.lua;' .. repo_root .. '/lua/?/init.lua;' .. package.path

local ok_version, version = pcall(require, 'ffr.utils.version')
if not ok_version then
  io.stderr:write('Error: could not load ffr.utils.version: ' .. tostring(version) .. '\n')
  os.exit(1)
end

local info, err = version.resolve(repo_root)
if not info then
  io.stderr:write('Error: ' .. (err or 'unknown') .. '\n')
  os.exit(1)
end

print('version=' .. info.version)
print('release_tag=' .. info.release_tag)
print('is_release=' .. tostring(info.is_release))

local github_output = os.getenv('GITHUB_OUTPUT')
if github_output and github_output ~= '' then
  local f = io.open(github_output, 'a')
  if f then
    f:write('version=' .. info.version .. '\n')
    f:write('release_tag=' .. info.release_tag .. '\n')
    f:write('is_release=' .. tostring(info.is_release) .. '\n')
    f:close()
  end
end
