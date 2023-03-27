#!/bin/zsh

export MUSICBEE_INSTALL=~/Documents/executables/musicbee/.wine 

# variables
export WINEARCH=win32
export MUSICBEE_PLUGIN_LOCATION=$MUSICBEE_INSTALL/drive_c/Program\ Files/MusicBee/Plugins

# compile
WINEPREFIX="$(pwd)/../.wine" wine "C:/windows/Microsoft.NET/Framework/v4.0.30319/csc.exe" /target:library *.cs

# copy over
cp mb_LinuxMediaControls.dll $MUSICBEE_PLUGIN_LOCATION

# restart
kill $(ps -A | rg "(\d+) .* MusicBee.exe" --replace '$1')
WINEPREFIX=$MUSICBEE_INSTALL wine C:\\Program\ Files\\MusicBee\\MusicBee.exe &
