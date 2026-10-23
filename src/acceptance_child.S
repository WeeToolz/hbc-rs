.section .rodata.hbc_acceptance_child,"a",@progbits
.balign 4
.globl hbc_acceptance_child_start
hbc_acceptance_child_start:
    b .Lchild_code
    .long 0x5f617267
    .space 28
.Lchild_code:
    li 4,1
    lis 5,0x8001
    lwz 6,0x2000(5)
    lis 7,0x1234
    ori 7,7,0x5678
    cmpw 6,7
    bne .Lterminal
    lwz 6,0x2004(5)
    cmpwi 6,0
    bne .Lterminal
    lwz 6,0x3000(5)
    cmpwi 6,0
    bne .Lterminal
    lwz 6,8(3)
    lis 7,0x5f61
    ori 7,7,0x7267
    cmpw 6,7
    bne .Lterminal
    lwz 6,12(3)
    lis 7,0x8132
    ori 7,7,0xfc00
    cmpw 6,7
    bne .Lterminal
    lwz 6,0(6)
    lis 7,0x626f
    ori 7,7,0x6f74
    cmpw 6,7
    bne .Lterminal
    lwz 6,16(3)
    cmpwi 6,10
    bne .Lterminal
    li 4,0
    b .Lterminal
.org 0xf0
.Lterminal:
    b .Lterminal
.org 0x100
.globl hbc_acceptance_child_end
hbc_acceptance_child_end:
