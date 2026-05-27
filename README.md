# jk

> Transparent CLI alias runner - a small, no-magic alternative to `just` / `make`.

You have shell incantations you keep copy-pasting from notes - project builds, container runs, ffmpeg pipelines. Put them in a `.jk` file once, run them by name:

```sh
# Before - copy-paste from notes.md every time
$ ffmpeg -i in.mp4 -c:v libx264 -preset slow -crf 22 -c:a aac -b:a 128k out.mp4

# After - same exact command, aliased
$ jk x264enc in.mp4 out.mp4
```

The string you wrote in `.jk`, after placeholder substitution, is what gets handed to your shell. No DSL, no command rewriting, no wrapper magic or implicit behaviour.

## Example

`.jk` in your project root (TOML):

```toml
shell = "bash"

[build]
desc = "build in release"
cmd = "cargo build --release"

[update]
desc = "apt update + upgrade"
cmd = [
  "apt update",
  "apt upgrade -y",
]

[x264enc]
desc = "h264 + aac with sensible defaults"
cmd = "ffmpeg -i #{1} -c:v libx264 -preset slow -crf 22 -c:a aac -b:a 128k #{2}"
```

```sh
$ jk                  # list everything in the current .jk
$ jk build            # → bash -c 'cargo build --release'
$ jk update           # runs each item as its own child process
$ jk x264enc a b      # #{1}/#{2} substituted from argv
$ jk build ++dry-run  # print the exact command without executing
```

## More patterns

Multi-line - folded to a single line at run time

```toml
[docker-sh]
cmd = '''
docker run --rm -it
  -v $(pwd):/work
  -w /work
  alpine sh
'''
# jk docker-sh
```

Namespaces (any depth) - `jk encode` lists encode/*

```toml
[encode.jpg]
cmd = "magick #{1} -quality 90 #{2}"
# jk encode jpg in.tiff out.jpg
```

All remaining args

```toml
[fmt]
cmd = "rustfmt #{@}"
# jk fmt src/main.rs src/lib.rs
```

Raw substitution - value dropped in without shell-quoting (`#{N!}` / `#{@!}`)

```toml
[vfilter]
cmd = "ffmpeg -i in.mp4 -vf '#{1!}' out.mp4"
# jk vfilter 'scale=640:480'
```

Per-leaf shell override

```toml
[winproc]
shell = "pwsh"
cmd = "Get-Process | Select-Object Name, CPU"
```

## Install

**Binary** - download from [Releases](https://github.com/Elypha/jk/releases).

**From source** (stable Rust):

```sh
cargo install --git https://github.com/Elypha/jk --locked
```

## Notes

- You name the shell (`bash`, `pwsh`, `fish`, `zsh`, `sh`) per file, with optional per-leaf override.
- Each sequence item is an independent child process - state (cwd, `$(...)` results, shell variables) does **not** carry between items.
- Child-shell exit codes pass through losslessly, so `jk a && jk b` composes naturally.
- jk looks for `.jk` walking up from cwd. `~/.jk/config.toml` provides global commands; local entries with the same name override global.

jk's own flags (all listed below) are prefixed `++` so they never collide with your underlying command's flags:

- `++dry-run` - print the rendered strings without executing
- `++version` - print version and exit
- `++config=<path>` - use this file instead of cwd walk-up
- `--` - end-of-flags separator (anything after is positional)

## License

[Apache-2.0](LICENSE)
