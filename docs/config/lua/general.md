# Lua Reference

Shelldone provides Lua 5.4 as a configuration language. This section documents
the various lua functions and types that are provided to the configuration
file. These are provided by the `shelldone` module that must be imported into
your configuration file:

```lua
local shelldone = require 'shelldone'
local config = {}
config.font = shelldone.font 'JetBrains Mono'
return config
```

## Full List of Configuration Options

[Config Options](config/index.md) has a list of the main configuration options.

