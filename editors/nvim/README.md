# Neovim integration

This plugin integrates `m1-lint` with [nvim-lint](https://github.com/mfussenegger/nvim-lint).

## Requirements

- Neovim 0.9+
- [nvim-lint](https://github.com/mfussenegger/nvim-lint)
- Rust toolchain (for the `cargo build --release` build step)

## lazy.nvim spec

```lua
{
  'C-Nucifora/m1-lint',
  build = 'cargo build --release',
  dependencies = { 'mfussenegger/nvim-lint' },
  config = function()
    require('m1_lint').setup({})
  end,
}
```

## Auto-lint behaviour

By default the linter runs on `BufWritePost` and `InsertLeave` for `*.m1scr` files. To disable this and trigger linting manually, pass `auto_lint = false`:

```lua
require('m1_lint').setup({ auto_lint = false })
```

When `auto_lint` is false, call `require('lint').try_lint()` yourself — for example from a keybinding or your own autocmd.

## Options

`setup()` accepts an optional table:

| Key | Type | Description |
|-----|------|-------------|
| `auto_lint` | `boolean` | Lint on `BufWritePost`/`InsertLeave` (default: `true`) |
| `linter` | `table` | Merged into the nvim-lint linter definition via `tbl_deep_extend` |

## Exit codes

`m1-lint` exits with:

- `0` — no diagnostics
- `1` — one or more lint findings
- `2` — invocation error (bad flags, unreadable file)

The plugin sets `ignore_exitcode = true` so exit 1 is treated as a normal linting result rather than a process failure.
