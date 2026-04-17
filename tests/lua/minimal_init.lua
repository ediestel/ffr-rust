-- Minimal init for headless test runs
vim.opt.runtimepath:prepend(vim.fn.getcwd())

-- Try to add plenary to rtp if available
local plenary_path = vim.fn.expand("~/.local/share/nvim/lazy/plenary.nvim")
if vim.fn.isdirectory(plenary_path) == 1 then
  vim.opt.runtimepath:prepend(plenary_path)
end
