#compdef alacritty

local ign

(( $#words > 2 )) && ign='!'
_arguments \
  "$ign(-)"{-h,--help}"[print help information]" \
  "--print-events[print all events to stdout]" \
  '(-v)'{-q,-qq}"[reduce the level of verbosity (min is -qq)]" \
  "--ref-test[generate ref test]" \
  "--hold[remain open after child process exits]" \
  '(-q)'{-v,-vv,-vvv}"[increase the level of verbosity (max is -vvv)]" \
  "$ign(-)"{-V,--version}"[print version information]" \
  "--class=[define the window class]:class" \
  "--embed=[define the X11 window ID (as a decimal integer) to embed Alacritty within]:windowId" \
  "(-e --command)"{-e,--command}"[execute command (must be last arg)]:program: _command_names -e:*::program arguments: _normal" \
  "--config-file=[specify an alternative config file]:file:_files" \
  "*"{-o=,--option=}"[override config file options]:option" \
  "(-t --title)"{-t=,--title=}"[define the window title]:title" \
  "--working-directory=[start shell in specified directory]:directory:_directories"
