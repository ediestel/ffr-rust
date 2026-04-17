local M = {}

function M.mkdir_recursive(path, callback)
  vim.uv.fs_stat(path, function(err, stat)
    if not err and stat then
      callback(true, nil)
      return
    end

    local parent = vim.fn.fnamemodify(path, ':h')
    if parent == path or parent == '' or parent == '.' then
      callback(false, 'Cannot create root directory')
      return
    end

    M.mkdir_recursive(parent, function(parent_ok, parent_err)
      if not parent_ok then
        callback(false, parent_err)
        return
      end

      vim.uv.fs_mkdir(path, 493, function(mkdir_err)
        if mkdir_err and not mkdir_err:match('EEXIST') then
          callback(false, 'Failed to create directory: ' .. mkdir_err)
          return
        end
        callback(true, nil)
      end)
    end)
  end)
end

function M.ensure_dir(path)
  if vim.fn.isdirectory(path) == 1 then
    return true
  end
  return vim.fn.mkdir(path, 'p') == 1
end

function M.exists(path)
  return vim.uv.fs_stat(path) ~= nil
end

function M.is_dir(path)
  local stat = vim.uv.fs_stat(path)
  return stat ~= nil and stat.type == 'directory'
end

function M.is_file(path)
  local stat = vim.uv.fs_stat(path)
  return stat ~= nil and stat.type == 'file'
end

function M.file_size(path)
  local stat = vim.uv.fs_stat(path)
  return stat and stat.size or 0
end

function M.readable_size(bytes)
  local units = { 'B', 'KB', 'MB', 'GB' }
  local i = 1
  while bytes >= 1024 and i < #units do
    bytes = bytes / 1024
    i = i + 1
  end
  return string.format('%.1f %s', bytes, units[i])
end

return M
