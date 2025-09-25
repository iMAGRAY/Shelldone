---
title: shelldone.action_callback
tags:
 - keys
 - event
---

# `shelldone.action_callback(callback)`

{{since('20211204-082213-a66c61ee9')}}

This function is a helper to register a custom event and return an action triggering it.

It is helpful to write custom key bindings directly, without having to declare
the event and use it in a different place.

The implementation is essentially the same as:
```lua
function shelldone.action_callback(callback)
  local event_id = '...' -- the function generates a unique event id
  shelldone.on(event_id, callback)
  return shelldone.action.EmitEvent(event_id)
end
```

See [shelldone.on](./on.md) and [shelldone.action](./action.md) for more info on what you can do with these.


## Usage

```lua
local shelldone = require 'shelldone'

return {
  keys = {
    {
      mods = 'CTRL|SHIFT',
      key = 'i',
      action = shelldone.action_callback(function(win, pane)
        shelldone.log_info 'Hello from callback!'
        shelldone.log_info(
          'WindowID:',
          win:window_id(),
          'PaneID:',
          pane:pane_id()
        )
      end),
    },
  },
}
```
