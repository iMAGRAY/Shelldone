# CopyMode `MoveForwardWordEnd`

{{since('20230320-124340-559cb7b0')}}

Moves the CopyMode cursor position forward to the end of word.

```lua
local shelldone = require 'shelldone'
local act = shelldone.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'e',
        mods = 'NONE',
        action = act.CopyMode 'MoveForwardWordEnd',
      },
    },
  },
}
```

