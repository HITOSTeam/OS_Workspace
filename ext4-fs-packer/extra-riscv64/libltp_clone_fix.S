.text
.global clone
.type clone, @function

.equ SYS_clone, 220
.equ SYS_exit, 93
.equ EINVAL, 22

clone:
    beqz a0, 2f
    beqz a1, 2f

    li t0, -16
    and a1, a1, t0
    addi a1, a1, -16
    sd a0, 0(a1)
    sd a3, 8(a1)

    mv a0, a2
    mv a2, a4
    mv a3, a5
    mv a4, a6
    li a7, SYS_clone
    ecall

    beqz a0, 1f
    bltz a0, 3f
    ret

1:
    ld a1, 0(sp)
    ld a0, 8(sp)
    jalr a1
    li a7, SYS_exit
    ecall
    j .

2:
    li a0, -EINVAL

3:
    neg t0, a0
    addi sp, sp, -16
    sd ra, 8(sp)
    sd t0, 0(sp)
    call __errno_location
    ld t0, 0(sp)
    sw t0, 0(a0)
    li a0, -1
    ld ra, 8(sp)
    addi sp, sp, 16
    ret
