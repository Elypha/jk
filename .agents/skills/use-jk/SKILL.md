---
name: use-jk
description: Technical reference for the jk alias runner and its `.jk` TOML format. Use for jk CLI syntax, config discovery, namespaces, placeholders, shells, sequences, overrides, and quoting.
---

# jk / .jk reference

`jk` resolves a TOML leaf path, substitutes arguments, and runs its command through the configured shell.

## Config discovery

Local config priority:

1. `++config=<path>`
2. Non-empty `JK_CONFIG`
3. Nearest `.jk`, searched from the current directory through its parents

The global config is `~/.jk/config.toml`. It loads in addition to the local config. A local leaf replaces the global leaf at the same path. Leaf/namespace conflicts are errors.

## CLI

```text
jk                         List all commands
jk <namespace>             List one namespace
jk <path> [args]           Run one leaf
jk <path> [args] ++dry-run Print rendered commands
```

| Syntax | Effect |
| --- | --- |
| `++dry-run` | Print rendered commands without running them. |
| `++version` | Print the version and exit. |
| `++config=<path>` | Select the local config file. |
| `--` | End jk flag parsing. |

`++` flags can appear anywhere before `--`. Set `JK_QUIET=1` to suppress execution status and config paths. Set `JK_NO_COLOR=1` to disable colour.

## TOML

`.jk` files use TOML 1.1.

```toml
shell = "pwsh"

[build]
cmd = "cargo build --release"

[git.pull]
desc = "$ jk git pull"
cmd = [
  "git pull",
  "git submodule update --init --recursive",
]

[media.encode]
shell = "bash"
desc = [
  "$ jk media encode INPUT OUTPUT [FFMPEG_ARGS...]",
  "encode a video",
]
cmd = '''
ffmpeg -i #{1}
  #{@}
  #{2}
'''
```

```text
$ jk media encode in.mp4 out.mp4 -preset slow ++dry-run
ffmpeg -i in.mp4 -preset slow out.mp4
```

| Item | Form |
| --- | --- |
| Leaf | Table with `cmd`; optional `desc` and `shell` |
| Namespace | Table containing child tables only |
| `cmd` | Non-empty string or non-empty string array |
| `desc` | String or string array |
| `shell` | `bash`, `sh`, `zsh`, `pwsh`, or `fish` |

A leaf `shell` overrides the file `shell`. Each `desc` array item displays on a separate line.

Multiline commands are folded: blank lines are removed, each line is trimmed, and the lines join with spaces.

## Placeholders

| Syntax | Expansion |
| --- | --- |
| `#{N}` | Argument N, quoted for the selected shell |
| `#{@}` | Remaining arguments, quoted individually |
| `#{N!}` | Argument N without quoting |
| `#{@!}` | Remaining arguments without quoting |

Numbered placeholders are 1-based and contiguous across the leaf. Extra arguments require `#{@}` or `#{@!}`.

Within each sequence item, `#{@}` starts after that item's highest numbered placeholder. Quoted placeholders must not receive additional manual quotes.

## Execution

- Each `cmd` array item starts a separate child shell.
- Directory changes, variables, and shell state do not carry between items.
- The sequence stops at the first non-zero exit.
- `jk` returns the child exit code.
- Shell profiles are disabled where supported.
