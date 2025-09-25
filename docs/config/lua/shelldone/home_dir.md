---
title: shelldone.home_dir
tags:
 - utility
 - filesystem
---

# `shelldone.home_dir`

This constant is set to the home directory of the user running `shelldone`.

```lua
local shelldone = require 'shelldone'
shelldone.log_error('Home ' .. shelldone.home_dir)
```


