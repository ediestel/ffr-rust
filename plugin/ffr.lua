if vim.g.loaded_ffr == 1 then
  return
end

vim.g.loaded_ffr = 1

require("ffr.commands").create_user_commands()