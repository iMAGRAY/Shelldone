---
title: shelldone.format
tags:
 - utility
 - string
---

# `shelldone.format({})`

{{since('20210314-114017-04b7cedd')}}

`shelldone.format` can be used to produce a formatted string
with terminal graphic attributes such as bold, italic and colors.
The resultant string is rendered into a string with shelldone
compatible escape sequences embedded.

`shelldone.format` accepts a single array argument, where each
element is a `FormatItem`.

This example logs the text `Hello`, then the date/time, underlined, in purple
text on a blue background to the stderr of the shelldone process:

```lua
local shelldone = require 'shelldone'

local success, date, stderr = shelldone.run_child_process { 'date' }

shelldone.log_info(shelldone.format {
  { Attribute = { Underline = 'Single' } },
  { Foreground = { AnsiColor = 'Fuchsia' } },
  { Background = { Color = 'blue' } },
  { Text = 'Hello ' .. date .. ' ' },
  'ResetAttributes',
  { Text = 'this text has default attributes' },
})
```

Possible values for the `FormatItem` elements are:

* `{Text="Hello"}` - the text `Hello`. The string can be any string expression,
  including escape sequences that are not supported directly by
  `shelldone.format`.
* `{Attribute={Underline="None"}}` - disable underline
* `{Attribute={Underline="Single"}}` - enable single underline
* `{Attribute={Underline="Double"}}` - enable double underline
* `{Attribute={Underline="Curly"}}` - enable curly underline
* `{Attribute={Underline="Dotted"}}` - enable dotted underline
* `{Attribute={Underline="Dashed"}}` - enable dashed underline
* `{Attribute={Intensity="Normal"}}` - set normal intensity
* `{Attribute={Intensity="Bold"}}` - set bold intensity
* `{Attribute={Intensity="Half"}}` - set half intensity
* `{Attribute={Italic=true}}` - enable italics
* `{Attribute={Italic=false}}` - disable italics
* `{Foreground={AnsiColor="Black"}}` - set foreground color to one of the ansi color palette values (index 0-15) using one of the names `Black`, `Maroon`, `Green`, `Olive`, `Navy`, `Purple`, `Teal`, `Silver`, `Grey`, `Red`, `Lime`, `Yellow`, `Blue`, `Fuchsia`, `Aqua` or `White`.
* `{Foreground={Color="yellow"}}` - set foreground color to a named color or rgb value like `#ffffff`.
* `{Background={AnsiColor="Black"}}` - set the background color to an ansi color as per `Foreground` above.
* `{Background={Color="blue"}}` - set the background color to a named color or rgb value as per `Foreground` above.
* `"ResetAttributes"` - reset all attributes to default. {{since('20220807-113146-c2fee766', inline=True)}}

This example shows how to use arbitrary escape sequences to change the underline color:

```lua
local shelldone = require 'shelldone'
shelldone.log_info(shelldone.format {
  -- turn on underlines
  { Attribute = { Underline = 'Single' } },
  -- make the underline red
  { Text = '\x1b[58:2::255:0:0m' },
  -- and say hello
  { Text = 'hello' },
})
```
