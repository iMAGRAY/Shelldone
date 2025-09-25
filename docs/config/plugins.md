
## Introduction

<!-- See also https://github.com/shelldone/terminal/commit/e4ae8a844d8feaa43e1de34c5cc8b4f07ce525dd -->

A Shelldone plugin is a package of Lua files that provide
some predefined functionality not in the core product.

A plugin is distributed via a Git URL.

!!! Tip

    Michael Brusegard maintains a [list of plugins](https://github.com/michaelbrusegard/awesome-shelldone)

## Installing a Plugin

Brief example:

```lua
local shelldone = require 'shelldone'
local a_plugin = shelldone.plugin.require 'https://github.com/owner/repo'

local config = shelldone.config_builder()

a_plugin.apply_to_config(config)

return config
```

The plugin URL must use the `HTTPS` or `file` [protocol](https://git-scm.com/book/en/v2/Git-on-the-Server-The-Protocols).

When Shelldone clones the repo into the runtime directory the default branch (probably `main`)
is checked out and used as the plugin source.

Plugins can be configured, for example:

```lua
local shelldone = require 'shelldone'
local a_plugin = shelldone.plugin.require 'https://github.com/owner/repo'

local config = shelldone.config_builder()

local myPluginConfig = { enable = true, location = 'right' }

a_plugin.apply_to_config(config, myPluginConfig)

return config
```

!!! Note

    Consult the README for a particular plugin to discover any specific configuration options.

## Updating Plugins

When changes are published to a plugin repository they are not updated in the local Shelldone instance.

Run the command [`shelldone.plugin.update_all()`](lua/shelldone.plugin/update_all.md) to update all local plugins.

!!! Tip

    This can be run using the Lua REPL in [DebugOverlay](../troubleshooting.md#debug-overlay).

## Removing a Plugin

When a plugin is first referenced, [`shelldone.plugin.require()`](lua/shelldone.plugin/require.md) will clone the repo if it doesn't already
exist and store it in the runtime directory under `plugins/NAME` where
`NAME` is derived from the repo URL.

You can discover locations of the various plugins with [`shelldone.plugin.list()`](lua/shelldone.plugin/list.md).

To remove the plugin simply delete the appropriate plugin directory.

## Developing a Plugin

1. Create a local development repo
2. Add a file `plugin/init.lua`
3. `init.lua` must return a module that exports an `apply_to_config`
   function. This function must accept at least a config builder parameter, but may
   pass other parameters, or a Lua table with a `config` field that maps
   to a config build parameter
4. Add any other Lua code needed to fulfil the plugin feature set.
5. Add the plugin using a local file url e.g.
   ```lua
   local a_plugin = shelldone.plugin.require "file:///home/user/projects/myPlugin"
   ```

!!! Info
    When changes are made to the local project, [`shelldone.plugin.update_all()`](lua/shelldone.plugin/update_all.md) must be run
    to sync the changes into the Shelldone runtime directory for testing and use.

!!! Info
    This assumes development on the repo default branch (i.e. `main`). To use a different
    development branch see below.

### Managing a Plugin with Multiple Lua Modules

When `requiring` other Lua modules in your plugin the value of `package.path` needs to updated
with the location of the plugin. The plugin directory can be obtained by running
`shelldone.plugin.list()`. This function returns an array of triplets. e.g.

```
[
    {
        "component": "filesCssZssZssZsUserssZsdevelopersZsprojectssZsmysDsPlugin",
        "plugin_dir": "/Users/alec/Library/Application Support/shelldone/plugins/filesCssZssZssZsUserssZsalecsZsprojectssZsbarsDsshelldone",
        "url": "file:///Users/developer/projects/my.Plugin",
    },
]
```

The package path can then be updated with the value of `plugin_dir`. For example:

```lua
function findPluginPackagePath(myProject)
  local separator = package.config:sub(1, 1) == '\\' and '\\' or '/'
  for _, v in ipairs(shelldone.plugin.list()) do
    if v.url == myProject then
      return v.plugin_dir .. separator .. 'plugin' .. separator .. '?.lua'
    end
  end
  --- #TODO Add error fail here
end

package.path = package.path
  .. ';'
  .. findPluginPackagePath 'file:///Users/developer/projects/my.Plugin'
```

!!! Tip
    Review other published plugins to discover more details on how to structure a plugin project

## Making changes to a Existing Plugin

1. Remove the original plugin from Shelldone
1. Fork the plugin repo
1. Clone the repo to a local directory
1. Optionally set an `upstream` remote to the original plugin repo. This makes it easier it merge upstream changes
1. Create a new branch for development
1. Make the new branch the default branch with `git symbolic-ref HEAD refs/heads/mybranch`
1. Set the `plugin_dir` if required (some plugins hard code the value of the plugin directory).
1. Add the plugin to Shelldone using the file protocol

Proceed using the develop workflow above
