---
title: shelldone.config_dir
tags:
 - filesystem
---

# `shelldone.config_dir`

This constant is set to the path to the directory in which your `shelldone.lua`
configuration file was found.

```lua
local shelldone = require 'shelldone'
shelldone.log_error('Config Dir ' .. shelldone.config_dir)
```


