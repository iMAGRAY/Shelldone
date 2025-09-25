# `QuickSelect`

{{since('20210502-130208-bff6815d')}}

Activates [Quick Select Mode](../../../quickselect.md).

```lua
local shelldone = require 'shelldone'

config.keys = {
  { key = ' ', mods = 'SHIFT|CTRL', action = shelldone.action.QuickSelect },
}
```

See also [QuickSelectArgs](QuickSelectArgs.md)
