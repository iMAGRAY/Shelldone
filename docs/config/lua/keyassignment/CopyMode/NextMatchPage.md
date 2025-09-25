# CopyMode `NextMatchPage`

{{since('20220624-141144-bd1b7c5d')}}

Move the CopyMode/SearchMode selection to the next matching text on the next
page of the screen, if any.

```lua
local shelldone = require 'shelldone'
local act = shelldone.action

return {
  key_tables = {
    search_mode = {
      {
        key = 'PageDown',
        mods = 'CTRL',
        action = act.CopyMode 'NextMatchPage',
      },
    },
  },
}
```

