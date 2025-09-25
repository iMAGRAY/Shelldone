---
title: shelldone.pad_left
tags:
 - utility
 - string
---
# shelldone.pad_left(string, min_width)

{{since('20210502-130208-bff6815d')}}

Returns a copy of `string` that is at least `min_width` columns
(as measured by [shelldone.column_width](column_width.md)).

If the string is shorter than `min_width`, spaces are added to
the left end of the string.

For example, `shelldone.pad_left("o", 3)` returns `"  o"`.

See also: [shelldone.truncate_left](truncate_left.md), [shelldone.pad_right](pad_right.md).


