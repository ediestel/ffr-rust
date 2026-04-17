local M = {}

---@param listed? boolean
---@param scratch? boolean
---@return integer bufnr
function M.create_buffer(listed, scratch)
  local bufnr = vim.api.nvim_create_buf(listed ~= false, scratch == true)
  vim.api.nvim_set_option_value("bufhidden", "hide", { buf = bufnr })
  vim.api.nvim_set_option_value("swapfile", false, { buf = bufnr })
  vim.api.nvim_set_option_value("modifiable", true, { buf = bufnr })
  vim.api.nvim_set_option_value("readonly", false, { buf = bufnr })
  return bufnr
end

---@param bufnr integer
---@param path string
---@param mode '"full"'|'"chunked"'|'"preview"'
---@param session_id string|nil
function M.set_ffr_vars(bufnr, path, mode, session_id)
  vim.b[bufnr].ffr_path = path
  vim.b[bufnr].ffr_mode = mode
  vim.b[bufnr].ffr_session_id = session_id
end

---@param bufnr integer
---@param lines string[]
function M.set_lines(bufnr, lines)
  if not M.is_valid(bufnr) then
    return
  end
  vim.api.nvim_set_option_value("modifiable", true, { buf = bufnr })
  vim.api.nvim_buf_set_lines(bufnr, 0, -1, false, lines)
  vim.api.nvim_set_option_value("modified", false, { buf = bufnr })
end

---@param bufnr integer
---@param line1 integer
---@param line2 integer
---@param lines string[]
function M.replace_lines(bufnr, line1, line2, lines)
  if not M.is_valid(bufnr) then
    return
  end
  vim.api.nvim_set_option_value("modifiable", true, { buf = bufnr })
  vim.api.nvim_buf_set_lines(bufnr, line1, line2, false, lines)
  vim.api.nvim_set_option_value("modified", false, { buf = bufnr })
end

---@param bufnr integer
---@param modifiable boolean
function M.set_modifiable(bufnr, modifiable)
  if not M.is_valid(bufnr) then
    return
  end
  vim.api.nvim_set_option_value("modifiable", modifiable, { buf = bufnr })
end

---@param bufnr integer
---@param filetype string|nil
function M.set_filetype(bufnr, filetype)
  if not M.is_valid(bufnr) or not filetype or filetype == "" then
    return
  end
  vim.api.nvim_set_option_value("filetype", filetype, { buf = bufnr })
end

---@param bufnr integer
---@param name string
function M.set_name(bufnr, name)
  if not M.is_valid(bufnr) or not name or name == "" then
    return
  end
  pcall(vim.api.nvim_buf_set_name, bufnr, name)
end

---@param bufnr integer
---@return integer? winid
function M.ensure_visible(bufnr)
  if not M.is_valid(bufnr) then
    return nil
  end

  for _, winid in ipairs(vim.api.nvim_list_wins()) do
    if vim.api.nvim_win_get_buf(winid) == bufnr then
      vim.api.nvim_set_current_win(winid)
      return winid
    end
  end

  vim.api.nvim_set_current_buf(bufnr)
  return vim.api.nvim_get_current_win()
end

---@param bufnr integer
---@return boolean
function M.is_valid(bufnr)
  return type(bufnr) == "number" and vim.api.nvim_buf_is_valid(bufnr)
end

return M