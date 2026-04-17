--- Dynamic loader for the ffr-nvim CDylib.
--- Mirrors the fff pattern: probe platform-specific paths, loadlib, graceful fallback.

local M = {}

local loaded_module = nil
local load_error = nil

local function lib_name()
  local os = jit and jit.os:lower() or vim.loop.os_uname().sysname:lower()
  if os == "osx" or os == "darwin" then
    return "libffr_nvim.dylib"
  elseif os == "windows" then
    return "ffr_nvim.dll"
  else
    return "libffr_nvim.so"
  end
end

local function probe_paths()
  local plugin_root = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":h:h:h:h")
  local name = lib_name()
  return {
    plugin_root .. "/target/release/" .. name,
    plugin_root .. "/target/debug/" .. name,
  }
end

local function try_load()
  if loaded_module then
    return loaded_module, nil
  end
  if load_error then
    return nil, load_error
  end

  local paths = probe_paths()
  for _, path in ipairs(paths) do
    if vim.fn.filereadable(path) == 1 then
      local loader, err = package.loadlib(path, "luaopen_ffr_nvim")
      if loader then
        local ok, mod = pcall(loader)
        if ok and type(mod) == "table" then
          loaded_module = mod
          return mod, nil
        else
          load_error = "failed to initialize ffr_nvim: " .. tostring(mod)
        end
      else
        load_error = "loadlib failed: " .. tostring(err)
      end
    end
  end

  if not load_error then
    load_error = "ffr-nvim binary not found. Build with: cargo build --release -p ffr-nvim"
  end

  return nil, load_error
end

---@return table|nil module, string|nil err
function M.get()
  return try_load()
end

---@return boolean
function M.is_available()
  local mod, _ = try_load()
  return mod ~= nil
end

return M
