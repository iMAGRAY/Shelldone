---
title: shelldone.to_string
tags:
 - utility
---
# `shelldone.to_string(arg)`

{{since('20240127-113634-bbcac864')}}

This function returns a string representation of any Lua value. In particular
this can be used to get a string representation of a table or userdata.

The intended purpose is as a human readable way to inspect lua values.  It is not machine
readable; do not attempt to use it as a serialization format as the format is not guaranteed
to remain the same across different versions of shelldone.

This same representation is used in the [debug overlay](../keyassignment/ShowDebugOverlay.md)
when printing the result of an expression from the Lua REPL and for the implicit string
conversions of the parameters passed to [shelldone.log_info](log_info.md).

```lua
local shelldone = require 'shelldone'
assert(shelldone.to_string { 1, 2 } == [=[[
    1,
    2,
]]=])
assert(shelldone.to_string { a = 1, b = 2 } == [[{
    "a": 1,
    "b": 2,
}]])
```

