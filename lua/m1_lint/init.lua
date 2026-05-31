local M = {}

-- Parse one line of m1-lint text output into an nvim-lint diagnostic table.
local function parse_line(line, _)
  -- pattern: <file>:<line>:<col>: <severity>[<code>]: <message>
  local lnum, col, sev, code, msg =
    line:match("^[^:]+:(%d+):(%d+): (%a+)%[([A-Z0-9]+)%]: (.+)$")
  if not lnum then
    return nil
  end
  local severity = sev:lower() == "error" and vim.diagnostic.severity.ERROR
    or vim.diagnostic.severity.WARN
  return {
    lnum = tonumber(lnum) - 1,
    col = tonumber(col) - 1,
    severity = severity,
    message = string.format("[%s] %s", code, msg),
    source = "m1-lint",
  }
end

function M.setup(opts)
  opts = opts or {}
  local plugin_dir = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":h:h:h")
  local bin = plugin_dir .. "/target/release/m1-lint"

  local ok, lint = pcall(require, "lint")
  if not ok then
    vim.notify("m1_lint: nvim-lint not found", vim.log.levels.WARN)
    return
  end

  lint.linters.m1_lint = vim.tbl_deep_extend("force", {
    name = "m1-lint",
    cmd = bin,
    stdin = false,
    args = {},
    -- m1-lint writes human output to stderr
    stream = "stderr",
    ignore_exitcode = true, -- exit 1 means lint findings, not an invocation error
    parser = function(output, _)
      local diags = {}
      for _, line in ipairs(vim.split(output, "\n", { plain = true })) do
        local d = parse_line(line)
        if d then
          diags[#diags + 1] = d
        end
      end
      return diags
    end,
  }, opts.linter or {})

  -- Register linter for the filetype
  lint.linters_by_ft =
    vim.tbl_deep_extend("force", lint.linters_by_ft or {}, { m1scr = { "m1_lint" } })

  -- Auto-lint on save and InsertLeave if no autocmd already set
  if opts.auto_lint ~= false then
    vim.api.nvim_create_autocmd({ "BufWritePost", "InsertLeave" }, {
      pattern = "*.m1scr",
      callback = function()
        lint.try_lint()
      end,
    })
  end
end

return M
