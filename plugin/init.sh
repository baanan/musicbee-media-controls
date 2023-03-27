export WINEARCH=win32
export WINEPREFIX="$(pwd)/../.wine"
winecfg
winetricks --force dotnet48
