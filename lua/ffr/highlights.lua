--- Default highlight groups for ffr-rendered buffers.
---
--- Users may override via `config.highlights.*` (string = name of an existing
--- group to link to, false = leave unset).

local M = {}

local DEFAULTS = {
  FFRChunkBoundary = { link = 'NonText' },
  FFRPreviewHeader = { link = 'Title' },
  FFRRejectedBanner = { link = 'WarningMsg' },
  FFRBinaryHex = { link = 'Comment' },
  FFRStatusline = { link = 'StatusLine' },
}

function M.setup(overrides)
  overrides = overrides or {}

  for group, default_spec in pairs(DEFAULTS) do
    local user_key = group
      :gsub('FFR', '')
      :gsub('(%u)', function(c, pos) return (pos == 1 and '' or '_') .. c:lower() end)
    -- FFRChunkBoundary → chunk_boundary, FFRPreviewHeader → preview_header, etc.
    local override = overrides[user_key]
    if type(override) == 'string' and override ~= '' then
      vim.api.nvim_set_hl(0, group, { link = override, default = true })
    elseif override == false then
      -- skip; leave unset
    else
      vim.api.nvim_set_hl(0, group, vim.tbl_extend('keep', default_spec, { default = true }))
    end
  end
end

function M.list()
  return vim.tbl_keys(DEFAULTS)
end

return M
