---
title: shelldone.truncate_left
tags:
 - utility
 - string
---
# shelldone.truncate_left(string, max_width)

{{since('20210502-130208-bff6815d')}}

Returns a copy of `string` that is no longer than `max_width` columns
(as measured by [shelldone.column_width](column_width.md)).

Truncation occurs by removing excess characters from the left
end of the string.

For example, `shelldone.truncate_left("hello", 3)` returns `"llo"`.

See also: [shelldone.truncate_right](truncate_right.md), [shelldone.pad_right](pad_right.md).

