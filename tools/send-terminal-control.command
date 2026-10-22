#!/bin/zsh
# Send the validated command-enabled HBC through the reference Wiiload sender.
setopt pipefail
control_dir="${0:A:h}"
export WIILOAD="tcp:${HBC_WII:-10.0.0.242}"
/opt/devkitpro/tools/bin/wiiload "$control_dir/../target/hbc-rs-terminal-control.dol" > "$control_dir/../target/terminal-control-upload.log" 2>&1
control_result=$?
cat "$control_dir/../target/terminal-control-upload.log"
print "$control_result" > "$control_dir/../target/terminal-control-upload.status"
(( control_result == 0 )) || exit "$control_result"
/usr/bin/python3 "$control_dir/hbc-control.py" --timeout 45 wait > "$control_dir/../target/terminal-control-ready.log" 2>&1
control_result=$?
cat "$control_dir/../target/terminal-control-ready.log"
print "$control_result" > "$control_dir/../target/terminal-control-ready.status"
exit "$control_result"
