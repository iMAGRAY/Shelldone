---
title: shelldone.config_file
tags:
 - filesystem
---

# `shelldone.config_file`

{{since('20210502-130208-bff6815d')}}

This constant is set to the path to the `shelldone.lua` that is in use.

```lua
local shelldone = require 'shelldone'
shelldone.log_info('Config file ' .. shelldone.config_file)
```



