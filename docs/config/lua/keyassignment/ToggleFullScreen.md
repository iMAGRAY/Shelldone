# `ToggleFullScreen`

Toggles full screen mode for the current window.

```lua
local shelldone = require 'shelldone'

config.keys = {
  {
    key = 'n',
    mods = 'SHIFT|CTRL',
    action = shelldone.action.ToggleFullScreen,
  },
}
```

See also: [native_macos_fullscreen_mode](../config/native_macos_fullscreen_mode.md).

