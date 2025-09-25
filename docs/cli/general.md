# Command Line

This section documents the shelldone command line.

*Note that `shelldone --help` or `shelldone SUBCOMMAND --help` will show the precise
set of options that are applicable to your installed version of shelldone.*

shelldone is deployed with two major executables:

* `shelldone` (or `shelldone.exe` on Windows) - for interacting with shelldone from the terminal
* `shelldone-gui` (or `shelldone-gui.exe` on Windows) - for spawning shelldone from a desktop environment

You will typically use `shelldone` when scripting shelldone; it knows when to
delegate to `shelldone-gui` under the covers.

If you are setting up a launcher for shelldone to run in the Windows GUI
environment then you will want to explicitly target `shelldone-gui` so that
Windows itself doesn't pop up a console host for its logging output.

!!! note
    `shelldone-gui.exe --help` will not output anything to a console when
    run on Windows systems, because it runs in the Windows GUI subsystem and has no
    connection to the console.  You can use `shelldone.exe --help` to see information
    about the various commands; it will delegate to `shelldone-gui.exe` when
    appropriate.

## Synopsis

```console
{% include "../examples/cmd-synopsis-shelldone--help.txt" %}
```
