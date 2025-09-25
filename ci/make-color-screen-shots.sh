#!/bin/bash

# Use eg: `xwininfo -int` to get the id of a shelldone
# and pass it to this script
WINID=$1

cd ../iTerm2-Color-Schemes/dynamic-colors
shots=../../shelldone/docs/colorschemes

printf "\e]0;shelldone\e\\"

for scheme in *.sh ; do
  clear
  echo $scheme
  prefix=$shots/$(echo $scheme | cut -c1 | tr '[:upper:]' '[:lower:]')
  mkdir -p $prefix
  bash "./$scheme"
  bash "../tools/screenshotTable.sh"
  sleep 0.2
  xwd -id $WINID | convert "xwd:-" "png:$prefix/${scheme%.sh}.png"
done

