#!/bin/zsh
# Open this file with the signed-in Terminal app for Wii network access.
setopt pipefail
control_dir="${0:A:h}"
if (( $# == 0 )); then
  /usr/bin/python3 "$control_dir/hbc-control.py" --help
  print
  read -r 'control_line?Enter one command and its arguments (or press Return to exit): '
  [[ -n "$control_line" ]] || exit 0
  control_arguments=(${(z)control_line})
else
  control_arguments=("$@")
fi
mkdir -p "$control_dir/../target" || exit 1
control_log="$control_dir/../target/terminal-control-$(date +%Y%m%d-%H%M%S)-$$.log"
/usr/bin/python3 "$control_dir/hbc-control.py" "${control_arguments[@]}" 2>&1 | tee "$control_log" "$control_dir/../target/terminal-control.log"
control_result=${pipestatus[1]}
print "$control_result" > "$control_log.status"
print "$control_result" > "$control_dir/../target/terminal-control.status"
print "Command exit status: $control_result"
exit "$control_result"
