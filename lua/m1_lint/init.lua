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
      local errors = {}
      for _, line in ipairs(vim.split(output, "\n", { plain = true })) do
        local d = parse_line(line)
        if d then
          diags[#diags + 1] = d
        elseif line:match("^error: ") or line:match("^warning: could not ") then
          -- m1-lint writes invocation failures (unreadable file, bad args,
          -- I/O errors) to stderr as `error:` / `warning: could not …`. These
          -- don't match the diagnostic pattern, so without this they'd be
          -- dropped and the user would see zero diagnostics and assume the
          -- file is clean. Surface them instead (#12). nvim-lint's parser
          -- callback isn't handed the process exit code, so detecting these
          -- stderr lines is the reliable signal that the linter itself failed.
          errors[#errors + 1] = line
        end
      end
      if #errors > 0 then
        -- Defer out of the parser callback; notify isn't safe mid-parse.
        vim.schedule(function()
          vim.notify(
            "m1-lint failed:\n" .. table.concat(errors, "\n"),
            vim.log.levels.ERROR
          )
        end)
      end
      return diags
    end,
  }, opts.linter or {})

  -- Register linter for the filetype. Append to the existing m1scr list rather
  -- than overwriting it with tbl_deep_extend("force", …), which would clobber a
  -- sibling linter (e.g. m1_typecheck) depending on plugin load order (#16).
  lint.linters_by_ft = lint.linters_by_ft or {}
  local ft = lint.linters_by_ft.m1scr or {}
  if not vim.tbl_contains(ft, "m1_lint") then
    table.insert(ft, "m1_lint")
  end
  lint.linters_by_ft.m1scr = ft

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
