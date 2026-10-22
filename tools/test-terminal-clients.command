#!/bin/zsh
# Loopback transport tests run in Terminal because the agent sandbox blocks bind.
setopt pipefail
control_dir="${0:A:h}"
cd "$control_dir/.." || exit 1
/usr/bin/python3 -m unittest discover -s tools -p 'test_hbc_*.py' > target/terminal-client-tests.log 2>&1
control_result=$?
cat target/terminal-client-tests.log
print "Test exit status: $control_result"
exit "$control_result"
