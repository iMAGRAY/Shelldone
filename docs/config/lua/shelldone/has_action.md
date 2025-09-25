---
title: shelldone.has_action
tags:
 - utility
 - version
---

# shelldone.has_action(NAME)

{{since('20230408-112425-69ae8472')}}

Returns true if the string *NAME* is a valid key assignment action variant
that can be used with [shelldone.action](action.md).

This is useful when you want to use a shelldone configuration across multiple
different versions of shelldone.

```lua
if shelldone.has_action 'PromptInputLine' then
  table.insert(config.keys, {
    key = 'p',
    mods = 'LEADER',
    action = shelldone.action.PromptInputLine {
      -- other parameters here
    },
  })
end
```
