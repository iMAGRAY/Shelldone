# `ActivateWindow(n)`

{{since('20230320-124340-559cb7b0')}}

Activates the *nth* GUI window, zero-based.

Performing this action is equivalent to executing this lua code fragment:

```lua
shelldone.gui.gui_windows()[n + 1]:focus()
```

Here's an example of setting up hotkeys to activate specific windows:

```lua
local shelldone = require 'shelldone'
local act = shelldone.action
local config = {}

config.keys = {}
for i = 1, 8 do
  -- CMD+ALT + number to activate that window
  table.insert(config.keys, {
    key = tostring(i),
    mods = 'CMD|ALT',
    action = act.ActivateWindow(i - 1),
  })
end

return config
```


See also 
[ActivateWindowRelative](ActivateWindowRelative.md),
[ActivateWindowRelativeNoWrap](ActivateWindowRelativeNoWrap.md).
