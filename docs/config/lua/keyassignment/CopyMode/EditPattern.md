# CopyMode `EditPattern`

{{since('20220624-141144-bd1b7c5d')}}

Put CopyMode/SearchMode into editing mode: keyboard input will be directed to
the search pattern editor.

```lua
local shelldone = require 'shelldone'
local act = shelldone.action

return {
  key_tables = {
    search_mode = {
      -- This action is not bound by default in shelldone
      { key = 'e', mods = 'CTRL', action = act.CopyMode 'EditPattern' },
    },
  },
}
```

See also [AcceptPattern](AcceptPattern.md).
