# `color:contrast_ratio(color)`

{{since('20220807-113146-c2fee766')}}

Computes the contrast ratio between the two colors.

```
> shelldone.color.parse("red"):contrast_ratio(shelldone.color.parse("yellow"))
1
> shelldone.color.parse("red"):contrast_ratio(shelldone.color.parse("navy"))
1.8273614734023298
```

The contrast ratio is computed by first converting to HSL, taking the
L components, and diving the lighter one by the darker one.

A contrast ratio of 1 means no contrast.

The maximum possible contrast ratio is 21:

```
> shelldone.color.parse("black"):contrast_ratio(shelldone.color.parse("white"))
21
```

