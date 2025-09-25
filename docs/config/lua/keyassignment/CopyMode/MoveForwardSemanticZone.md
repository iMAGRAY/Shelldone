# CopyMode `MoveForwardSemanticZone`

{{since('20220903-194523-3bb1ed61')}}

Moves the CopyMode cursor position one semantic zone to the right.

See [Shell Integration](../../../../shell-integration.md) for more information
about semantic zones.

```lua
local shelldone = require 'shelldone'
local act = shelldone.action

return {
  key_tables = {
    copy_mode = {
      {
        key = 'Z',
        mods = 'NONE',
        action = act.CopyMode 'MoveForwardSemanticZone',
      },
    },
  },
}
```


