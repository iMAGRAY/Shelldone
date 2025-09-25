# `QuitApplication`

Terminate the Shelldone application, killing all tabs.

```lua
local shelldone = require 'shelldone'

config.keys = {
  { key = 'q', mods = 'CMD', action = shelldone.action.QuitApplication },
}
```


