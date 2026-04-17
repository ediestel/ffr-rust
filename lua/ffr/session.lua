local M = {}

local sessions = {}
local by_bufnr = {}

---@class FFRSession
---@field id string
---@field path string
---@field mode '"full"'|'"chunked"'|'"preview"'
---@field bufnr integer
---@field winid integer|nil
---@field classification table
---@field current_chunk integer|nil
---@field loaded_chunks table<integer, boolean>
---@field line_index_ready boolean
---@field eof boolean
---@field revision string
---@field last_rendered_range table|nil

---@param session FFRSession
---@return FFRSession
function M.create(session)
  sessions[session.id] = session
  by_bufnr[session.bufnr] = session.id
  return session
end

---@param session_id string
---@return FFRSession|nil
function M.get(session_id)
  return sessions[session_id]
end

---@param bufnr integer
---@return FFRSession|nil
function M.get_by_bufnr(bufnr)
  local session_id = by_bufnr[bufnr]
  if not session_id then
    return nil
  end
  return sessions[session_id]
end

---@param session_id string
---@return boolean
function M.has(session_id)
  return sessions[session_id] ~= nil
end

---@param session_id string
---@return boolean
function M.destroy(session_id)
  local session = sessions[session_id]
  if not session then
    return false
  end

  by_bufnr[session.bufnr] = nil
  sessions[session_id] = nil
  return true
end

---@param session_id string
---@param patch table
---@return FFRSession|nil
function M.update(session_id, patch)
  local session = sessions[session_id]
  if not session then
    return nil
  end

  for k, v in pairs(patch) do
    session[k] = v
  end

  return session
end

---@return FFRSession|nil
function M.current()
  return M.get_by_bufnr(vim.api.nvim_get_current_buf())
end

---@param bufnr integer
---@param session_id string
function M.attach_bufnr(bufnr, session_id)
  by_bufnr[bufnr] = session_id
end

---@param bufnr integer
function M.detach_bufnr(bufnr)
  by_bufnr[bufnr] = nil
end

return M