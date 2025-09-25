---
title: shelldone.strftime
tags:
 - utility
 - time
 - string
---
# `shelldone.strftime(format)`

{{since('20210314-114017-04b7cedd')}}

Formats the current local date/time into a string using [the Rust chrono
strftime syntax](https://docs.rs/chrono/0.4.19/chrono/format/strftime/index.html).

```lua
local shelldone = require 'shelldone'

local date_and_time = shelldone.strftime '%Y-%m-%d %H:%M:%S'
shelldone.log_info(date_and_time)
```

See also [strftime_utc](strftime_utc.md) and [shelldone.time](../shelldone.time/index.md).

