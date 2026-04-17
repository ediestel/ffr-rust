-- Luacheck config for ffr.nvim.
-- Tuned for Neovim Lua: treat `vim` and friends as global, ignore length of
-- generated strings, and silence the docstring-heavy @class comments.

std = "lua51+busted"
cache = true

globals = { "vim", "describe", "it", "before_each", "after_each", "assert_" }

-- Max line length — match stylua's column_width.
max_line_length = 120

-- Silence: unused 'self' on module functions, and shadowing in local blocks.
self = false
ignore = {
  "631", -- line too long (handled by stylua)
}

exclude_files = {
  "target/",
  "packages/ffr-bin-*/",
  "tests/lua/minimal_init.lua",
}
