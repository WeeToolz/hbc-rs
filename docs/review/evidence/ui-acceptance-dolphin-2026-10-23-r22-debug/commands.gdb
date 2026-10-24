set pagination off
set language c
target remote localhost:55287
break *0x801079e4
commands
silent
printf "OUTPUT slot=%u len=%u\n", $r4, $r6
x/8bx $r5
continue
end
continue
bt
p 'wpad_sys::powerpc_backend::SLOTS'[0]
p 'wpad_sys::WPAD_DATA_STATE'[0]
quit
