# jk

> Transparent CLI alias runner - a small, no-magic alternative to `just` / `make`.

You have shell incantations you keep copy-pasting from notes - project builds, container runs, ffmpeg pipelines. Put them in a `.jk` file once, run them by name:

```sh
# Before - copy-paste from notes.md every time
$ ffmpeg -i in.mp4 -c:v libx264 -preset slow -crf 22 -c:a aac -b:a 128k out.mp4

# After - same exact command, aliased
$ jk media x264 in.mp4 out.mp4 -preset slow -crf 22
```

The string you wrote in `.jk`, after placeholder substitution, is what gets handed to your shell. No DSL, no command rewriting, no wrapper magic or implicit behaviour.

## Quick start

Run `jk ++init` in your project root, or create `.jk` manually. The file uses TOML 1.1:

```toml
#:schema https://raw.githubusercontent.com/Elypha/jk/master/schema/jk.schema.json

shell = "bash"

[build]
cmd = "cargo build --release"

[media.x264]
desc = "$ jk media x264 INPUT OUTPUT [FFMPEG_ARGS...]"
cmd = '''
ffmpeg -i #{1}
    -c:v libx264
    #{@}
    -c:a aac -b:a 128k
    #{2}
'''
```

```sh
$ jk                                      # list all commands
$ jk media                                # list media/*
$ jk build ++dry-run
cargo build --release
$ jk media x264 in.mp4 out.mp4 -preset slow ++dry-run
ffmpeg -i in.mp4 -c:v libx264 -preset slow -c:a aac -b:a 128k out.mp4
```

Quick notes:

1. `shell` accepts `bash`, `sh`, `zsh`, `pwsh`, `fish`, or `bun`.
    - `bash` on Windows uses Git for Windows `<Git>/bin/bash.exe` (MSYS path conversion still applies).
    - `bash` on other platforms uses the native `bash` on `PATH`.
    - `pwsh` is .NET PowerShell Core (Windows/Linux/macOS), not Windows PowerShell (`powershell.exe`).
    - `bun` uses [Bun Shell](https://bun.com/docs/runtime/shell) through `bun exec`, providing a cross-platform, bash-like shell.
2. `desc` is used for command listings.
3. `#{1}` and `#{2}` select positional arguments, and `#{@}` inserts the rest.
    - jk quotes `#{}` injection for the selected shell. See [Raw shell syntax](#raw-shell-syntax) if you don't want quoting.

## More patterns

### Sequence and multiline description

```toml
[release]
desc = [
    "$ jk release VERSION",
    "test, then publish",
]
cmd = [
    "cargo test",
    "./release.sh #{1}",
]
```

Each item starts a new child shell. Shell state does not carry between items. The sequence stops at the first non-zero exit.

### Raw shell syntax

```toml
[log]
cmd = "git log #{1!}"
# jk log '--oneline | head -5'
```

`#{N!}` and `#{@!}` skip shell quoting. Use them when one argument must expand into shell syntax.

### Per-command shell

```toml
[processes]
shell = "pwsh"
cmd = "Get-Process | Select-Object Name, CPU"
```

A command-level value overrides the file-level value.

## Install

**Binary** (static)

Download from [Releases](https://github.com/Elypha/jk/releases).

**From source** - build with Rust stable

```sh
cargo install --git https://github.com/Elypha/jk --locked
```

**Agent skill** - install with the [skills CLI](https://github.com/vercel-labs/skills):

```sh
npx skills add Elypha/jk --skill use-jk -g
```

```sh
npx skills check
npx skills update use-jk -g
npx skills remove use-jk -g
```

## Notes

Tech details:

- Child-shell exit codes pass through losslessly, so `jk a && jk b` composes naturally.
- jk looks for `.jk` walking up from cwd. `~/.jk/config.toml` provides global commands; listings show global and local commands in separate, recursively expanded sections, with groups before individual commands. Local entries with the same name override global.
- Write long commands in a readable way. jk joins each `cmd` into a one-line string.

jk's own flags (all listed below) are prefixed `++` so they never collide with your underlying command's flags:

- `++dry-run` - print the rendered strings without executing
- `++init` - create a starter `.jk` in the current directory; never overwrite an existing file
- `++version` - print version and exit
- `++config=<path>` - use this file instead of cwd walk-up
- `--` - end-of-flags separator (anything after is positional)

## License

[Apache-2.0](LICENSE)
