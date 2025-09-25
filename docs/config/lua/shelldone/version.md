---
title: shelldone.version
tags:
 - utility
 - version
---
# `shelldone.version`

This constant is set to the `shelldone` version string that is also reported
by running `shelldone -V`.  This can potentially be used to adjust configuration
according to the installed version.

The version string looks like `20200406-151651-5b700e4`.  You can compare the
strings lexicographically if you wish to test whether a given version is newer
than another; the first component is the date on which the release was made,
the second component is the time and the final component is a git hash.

```lua
local shelldone = require 'shelldone'
shelldone.log_error('Version ' .. shelldone.version)
```


