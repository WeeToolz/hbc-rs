set pagination off
set language c
target remote localhost:55287
break *0x800eda78 if $r3 < 0x30
commands
silent
printf "INPUT len=%u\n", $r31
x/22bx $r15
continue
end
continue
p 'wpad_sys::WPAD_FORMAT'[0]
p 'wpad_sys::powerpc_backend::SLOTS'[0].initial_handshake
quit
