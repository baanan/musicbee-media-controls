#!/bin/zsh

export MUSICBEE_INSTALL=~/Documents/executables/musicbee/.wine 

# variables
export WINEARCH=win32
export MUSICBEE_PLUGIN_LOCATION=$MUSICBEE_INSTALL/drive_c/Program\ Files/MusicBee/Plugins

cp mb_LinuxMediaControls.dll $MUSICBEE_PLUGIN_LOCATION
