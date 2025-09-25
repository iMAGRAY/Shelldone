# Troubleshooting

## Review logs/error messages

If things aren't working out, there may be an issue printed in the logs.
Read on to learn more about how to see those logs.

### Debug Overlay

By default, pressing <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>L</kbd> will activate
the debug overlay and allow you to review the most recently logged issues.
It also gives you access to a Lua REPL for evaluating built-in lua functions.

See [ShowDebugOverlay](config/lua/keyassignment/ShowDebugOverlay.md) for more
information on this key assignment.

### Log Files

You can find log files in `$XDG_RUNTIME_DIR/shelldone` on unix systems,
or `$HOME/.local/share/shelldone` on macOS and Windows systems.

### Increasing Log Verbosity

The `SHELLDONE_LOG` environment variable can be used to adjust the level
of logging for different modules within shelldone.

To see maximum verbosity, you can start shelldone like this:

```
SHELLDONE_LOG=debug shelldone
```

to see debug level logs for everything on stdout.

On Windows systems you'll usually need to set the environment variable separately:

Using `cmd.exe`:

```
C:\> set SHELLDONE_LOG=debug
C:\> shelldone
```

Using powershell:

```
PS C:\> $env:SHELLDONE_LOG="debug"
PS C:\> shelldone
```

When using a flatpak you must first enter the flatpak container by running:

```
flatpak run --command=sh --devel net.shelldone.terminal
```

Before then running `shelldone`.

Each log line will include the module name, which is a colon separated
namespace; in the output below the modules are `config`,
`shelldone_gui::frontend`, `shelldone_font::ftwrap` and `shelldone_gui::termwindow`:

```
10:29:24.451  DEBUG  config                    > Reloaded configuration! generation=2
10:29:24.452  DEBUG  shelldone_gui::frontend     > workspace is default, fixup windows
10:29:24.459  DEBUG  shelldone_font::ftwrap      > set_char_size computing 12 dpi=124 (pixel height=20.666666666666668)
10:29:24.461  DEBUG  shelldone_font::ftwrap      > set_char_size computing 12 dpi=124 (pixel height=20.666666666666668)
10:29:24.494  DEBUG  shelldone_gui::termwindow   > FocusChanged(true)
10:29:24.495  DEBUG  shelldone_gui::termwindow   > FocusChanged(false)
```

Those modules generally match up to directories and file names within the
shelldone source code, or to external modules that shelldone depends upon.

You can set a more restrictive filter to focus in on just the things you want.
For example, if you wanted to debug only configuration related things you might
set:

```
SHELLDONE_LOG=config=debug,info
```

which says:

* log `config` at `debug` level
* everything else at `info` level

You can add more comma-separated items:

```
SHELLDONE_LOG=config=debug,shelldone_font=debug,info
```

See Rust's [env_logger
documentation](https://docs.rs/env_logger/latest/env_logger/#enabling-logging)
for more details on the syntax/possibilities.

## Debugging Keyboard Related issues

Turn on [debug_key_events](config/lua/config/debug_key_events.md) to log
information about key presses.

Use [shelldone show-keys](cli/show-keys.md) or `shelldone show-keys --lua` to show
the effective set of key and mouse assignments defined by your config.

Consider changing [use_ime](config/lua/config/use_ime.md) to see that is
influencing your keyboard usage.

Double check to see if you have some system level utility/software that might
be intercepting or changing the behavior of a keyboard shortcut that you're
trying to use.

## Debugging Font Display

Use `shelldone ls-fonts` to explain which fonts will be used for different styles
of text.

Use `shelldone ls-fonts --list-system` to get a list of fonts available on your
system, in a form that you can use in your config file.

Use `shelldone ls-fonts --text foo` to explain how shelldone will render the text
`foo`, and `shelldone ls-fonts --text foo --rasterize-ascii` to show an ascii art
rendition of that text.

