#!/bin/zsh
control_dir="${0:A:h}"
cd "$control_dir/.." || exit 1
/usr/bin/python3 tools/test_remote_input_dolphin.py --host 10.0.0.139 --dol target/hbc-rs-terminal-control.dol --control > target/terminal-dolphin-tests.log 2>&1
control_result=$?
cat target/terminal-dolphin-tests.log
print "Dolphin test exit status: $control_result"
exit "$control_result"
