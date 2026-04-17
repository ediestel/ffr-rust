local M = {}

local config = require("ffr.config")

---@alias FFRPolicyDecision
---| "full_text_open"
---| "chunked_text_open"
---| "preview_only"
---| "reject_open"
---| "specialized_handler"

---@param classify_result table
---@return boolean
function M.is_textual(classify_result)
  if not classify_result then
    return false
  end

  if classify_result.binary then
    return false
  end

  return classify_result.kind == "text"
    or classify_result.kind == "json"
    or classify_result.kind == "minified"
end

---@param stat_result table
---@param cfg table
---@return boolean
function M.allows_full_open(stat_result, cfg)
  if not stat_result or not stat_result.size then
    return false
  end
  return stat_result.size <= cfg.full_open_max_bytes
end

---@param classify_result table
---@param cfg table
---@return boolean
function M.is_minified_like(classify_result, cfg)
  if not classify_result then
    return false
  end

  if classify_result.kind == "minified" then
    return true
  end

  if classify_result.minified == true then
    return true
  end

  return false
end

---@param stat_result table
---@param classify_result table
---@param opts? table
---@return FFRPolicyDecision decision, string? reason
function M.decide(stat_result, classify_result, opts)
  local cfg = config.get()
  opts = opts or {}

  if not stat_result or stat_result.exists ~= true or stat_result.is_file ~= true then
    return "reject_open", "path is not a readable file"
  end

  if not classify_result then
    return "reject_open", "missing classification"
  end

  if classify_result.kind == "pdf"
    or classify_result.kind == "image"
    or classify_result.kind == "archive"
  then
    return "specialized_handler", classify_result.kind
  end

  if classify_result.binary then
    return "reject_open", classify_result.reason or "binary file"
  end

  if not M.is_textual(classify_result) then
    return "reject_open", classify_result.reason or "unsupported file kind"
  end

  -- fff integration: honor initial_mode override
  local initial_mode = opts.initial_mode
  if initial_mode == "preview" then
    return "preview_only", "initial_mode=preview"
  elseif initial_mode == "chunked" then
    if classify_result.preview_allowed ~= false then
      return "chunked_text_open", "initial_mode=chunked"
    end
  elseif initial_mode == "full" then
    return "full_text_open", "initial_mode=full"
  end
  -- initial_mode == "auto" or nil: fall through to standard logic

  if opts.preview == true then
    return "preview_only", "preview requested"
  end

  if M.is_minified_like(classify_result, cfg) then
    return "preview_only", "minified-like file"
  end

  if classify_result.too_large_for_full_open == true then
    if classify_result.preview_allowed == true then
      return "chunked_text_open", "too large for full open"
    end
    return "reject_open", classify_result.reason or "file too large"
  end

  if M.allows_full_open(stat_result, cfg) then
    return "full_text_open", nil
  end

  if classify_result.preview_allowed == true then
    return "chunked_text_open", "large text file"
  end

  return "reject_open", classify_result.reason or "no valid policy decision"
end

--- Dispatch a specialized-handler decision to the right Lua renderer.
--- Returns (bufnr, err) like the rest of the open pipeline.
---@param path string
---@param classify_result table
---@return integer|nil bufnr, string|nil err
function M.route_specialized(path, classify_result)
  local client = require("ffr.client")
  local content, spec_err = client.extract_specialized(path)
  if spec_err or not content then
    return nil, ("specialized handler failed: %s"):format(spec_err or "unknown")
  end

  local kind = tostring(content.kind or classify_result.kind or "unknown")
  local ok_h, handler = pcall(require, "ffr.handlers." .. kind)
  if not ok_h or not handler or type(handler.render) ~= "function" then
    return nil, ("no renderer for specialized kind: %s"):format(kind)
  end

  local bufnr = handler.render(path, content)
  return bufnr, nil
end

return M
