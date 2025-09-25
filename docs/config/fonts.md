### Font Related Configuration

Shelldone bundles [JetBrains Mono](https://www.jetbrains.com/lp/mono/),
[Nerd Font Symbols](https://nerdfonts.com) and
[Noto Color Emoji](https://www.google.com/get/noto/help/emoji/) fonts
and uses those for the default font configuration.

If you wish to use a different font face, then you can use
the [shelldone.font](lua/shelldone/font.md) function to specify it:

```lua
config.font = shelldone.font 'Fira Code'
-- You can specify some parameters to influence the font selection;
-- for example, this selects a Bold, Italic font variant.
config.font =
  shelldone.font('JetBrains Mono', { weight = 'Bold', italic = true })
```

#### Fallback

Shelldone allows specifying an ordered list of fonts; when resolving
text into glyphs the first font in the list is consulted, and if the
glyph isn't present in that font, Shelldone proceeds to the next font
in the fallback list.

The default fallback includes the popular [Nerd Font
Symbols](https://nerdfonts.com) font, which means that you don't need to use
specially patched fonts to use the powerline or Nerd Fonts symbol glyphs.

You can specify your own fallback; that's useful if you've got a killer
monospace font, but it doesn't have glyphs for the asian script that you
sometimes work with:

```lua
config.font = shelldone.font_with_fallback {
  'Fira Code',
  'DengXian',
}
```

Shelldone will still append its default fallback to whatever list you specify,
so you needn't worry about replicating that list if you set your own fallback.

If none of the fonts in the fallback list (including Shelldone's default fallback
list) contain a given glyph, then shelldone will resolve the system fallback list
and try those fonts too.  If a glyph cannot be resolved, shelldone will render a
special "Last Resort" glyph as a placeholder.  You may notice the placeholder
appear momentarily and then refresh itself to the system fallback glyph on some
systems.

### Font Related Options

Additional options for configuring fonts can be found elsewhere in the docs:

* [bold_brightens_ansi_colors](lua/config/bold_brightens_ansi_colors.md) - whether bold text uses the bright ansi palette
* [dpi](lua/config/dpi.md) - override the DPI; potentially useful for X11 users with high-density displays if experiencing tiny or blurry fonts
* [font_dirs](lua/config/font_dirs.md) - look for fonts in a set of directories
* [font_locator](lua/config/font_locator.md) - override the system font resolver
* [font_rules](lua/config/font_rules.md) - advanced control over which fonts are used for italic, bold and other textual styles
* [font_shaper](lua/config/font_shaper.md) - affects kerning and ligatures
* [font_size](lua/config/font_size.md) - change the size of the text
* [freetype_load_flags](lua/config/freetype_load_flags.md) - advanced hinting configuration
* [freetype_load_target](lua/config/freetype_load_target.md) - configure hinting and anti-aliasing
* [freetype_render_target](lua/config/freetype_render_target.md) - configure anti-aliasing
* [cell_width](lua/config/cell_width.md) - scale the font-specified cell width
* [line_height](lua/config/line_height.md) - scale the font-specified line height
* [shelldone.font](lua/shelldone/font.md) - select a font based on family and style attributes
* [shelldone.font_with_fallback](lua/shelldone/font_with_fallback.md) - select a font from a list of candidates

## Troubleshooting Fonts

You may use `shelldone ls-fonts` to have shelldone explain information about which font files it will use for the different text styles.

It shows output like this:

```console
$ shelldone ls-fonts
Primary font:
shelldone.font_with_fallback({
  -- /home/shelldone/.fonts/OperatorMonoSSmLig-Medium.otf, FontDirs
  {family="Operator Mono SSm Lig", weight="DemiLight"},

  -- /home/shelldone/.fonts/MaterialDesignIconsDesktop.ttf, FontDirs
  "Material Design Icons Desktop",

  -- /usr/share/fonts/jetbrains-mono-fonts/JetBrainsMono-Regular.ttf, FontConfig
  "JetBrains Mono",

  -- /usr/share/fonts/google-noto-emoji/NotoColorEmoji.ttf, FontConfig
  -- Assumed to have Emoji Presentation
  -- Pixel sizes: [128]
  "Noto Color Emoji",
})


When Intensity=Half Italic=true:
shelldone.font_with_fallback({
  -- /home/shelldone/.fonts/OperatorMonoSSmLig-BookItalic.otf, FontDirs
  {family="Operator Mono SSm Lig", weight=325, italic=true},

  -- /home/shelldone/.fonts/MaterialDesignIconsDesktop.ttf, FontDirs
  "Material Design Icons Desktop",

  -- /usr/share/fonts/jetbrains-mono-fonts/JetBrainsMono-Regular.ttf, FontConfig
  "JetBrains Mono",

  -- /usr/share/fonts/google-noto-emoji/NotoColorEmoji.ttf, FontConfig
  -- Assumed to have Emoji Presentation
  -- Pixel sizes: [128]
  "Noto Color Emoji",
})
...
```

You can ask shelldone to including a listing of all of the fonts on the system in a form that can be copied and pasted into the configuration file:

```console
$ shelldone ls-fonts --list-system
<same output as above, but then:>
112 fonts found in your font_dirs + built-in fonts:
shelldone.font("Cascadia Code", {weight="ExtraLight", stretch="Normal", italic=false}) -- /home/shelldone/.fonts/CascadiaCode.ttf index=0 variation=1, FontDirs
shelldone.font("Cascadia Code", {weight="Light", stretch="Normal", italic=false}) -- /home/shelldone/.fonts/CascadiaCode.ttf index=0 variation=2, FontDirs
shelldone.font("Cascadia Code", {weight="DemiLight", stretch="Normal", italic=false}) -- /home/shelldone/.fonts/CascadiaCode.ttf index=0 variation=3, FontDirs
shelldone.font("Cascadia Code", {weight="Regular", stretch="Normal", italic=false}) -- /home/shelldone/.fonts/CascadiaCode.ttf index=0 variation=4, FontDirs
shelldone.font("Cascadia Code", {weight="DemiBold", stretch="Normal", italic=false}) -- /home/shelldone/.fonts/CascadiaCode.ttf index=0 variation=5, FontDirs
shelldone.font("Cascadia Code", {weight="Bold", stretch="Normal", italic=false}) -- /home/shelldone/.fonts/CascadiaCode.ttf index=0 variation=6, FontDirs
shelldone.font("Fira Code", {weight="Light", stretch="Normal", italic=false}) -- /home/shelldone/.fonts/FiraCode-Light.otf, FontDirs
shelldone.font("Fira Code", {weight="Regular", stretch="Normal", italic=false}) -- /home/shelldone/.fonts/FiraCode-Regular.otf, FontDirs
shelldone.font("Fira Code", {weight=450, stretch="Normal", italic=false}) -- /home/shelldone/.fonts/FiraCode-Retina.otf, FontDirs
shelldone.font("Fira Code", {weight="Medium", stretch="Normal", italic=false}) -- /home/shelldone/.fonts/FiraCode-Medium.otf, FontDirs
shelldone.font("Fira Code", {weight="Bold", stretch="Normal", italic=false}) -- /home/shelldone/.fonts/FiraCode-Bold.otf, FontDirs
shelldone.font("Font Awesome 5 Free", {weight="Black", stretch="Normal", italic=false}) -- /home/shelldone/.fonts/Font Awesome 5 Free-Solid-900.otf, FontDirs
...
690 system fonts found using FontConfig:
shelldone.font("Abyssinica SIL", {weight="Regular", stretch="Normal", italic=false}) -- /usr/share/fonts/sil-abyssinica-fonts/AbyssinicaSIL-R.ttf, FontConfig
shelldone.font("C059", {weight="Regular", stretch="Normal", italic=false}) -- /usr/share/fonts/urw-base35/C059-Bold.t1, FontConfig
shelldone.font("C059", {weight="Regular", stretch="Normal", italic=false}) -- /usr/share/fonts/urw-base35/C059-Roman.otf, FontConfig
shelldone.font("C059", {weight="Regular", stretch="Normal", italic=false}) -- /usr/share/fonts/urw-base35/C059-Roman.t1, FontConfig
shelldone.font("C059", {weight="Regular", stretch="Normal", italic=true}) -- /usr/share/fonts/urw-base35/C059-BdIta.t1, FontConfig
shelldone.font("C059", {weight="Regular", stretch="Normal", italic=true}) -- /usr/share/fonts/urw-base35/C059-Italic.otf, FontConfig
...
```

You may also display the shaping plan for a given text string; in this example,
the `a` and the `b` are separated by a special symbol which is not present in
the main font, so we expect to see a different font used for that glyph:

```console
$ shelldone ls-fonts --text a🞄b
a    \u{61}       x_adv=8  glyph=29   shelldone.font("Operator Mono SSm Lig", {weight="DemiLight", stretch="Normal", italic=false})
                                      /home/shelldone/.fonts/OperatorMonoSSmLig-Medium.otf, FontDirs
🞄    \u{1f784}    x_adv=4  glyph=9129 shelldone.font("Symbola", {weight="Regular", stretch="SemiCondensed", italic=false})
                                      /usr/share/fonts/gdouros-symbola/Symbola.ttf, FontConfig
b    \u{62}       x_adv=8  glyph=30   shelldone.font("Operator Mono SSm Lig", {weight="DemiLight", stretch="Normal", italic=false})
                                      /home/shelldone/.fonts/OperatorMonoSSmLig-Medium.otf, FontDirs
```
