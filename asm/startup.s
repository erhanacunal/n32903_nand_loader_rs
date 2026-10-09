@/**************************************************************************//**
@ * @file     startup.s
@ * @brief    N32903 NAND Loader startup (ARM exception vectors + stack init)
@ *
@ * Ported from vectors.s + wb_init.s from Nuvoton N32905 NAND Loader
@ *****************************************************************************/

.section .vectors, "ax"
.global Vector_Table

Vector_Table:
    LDR     PC, Reset_Addr
    LDR     PC, Undefined_Addr
    LDR     PC, SWI_Addr
    LDR     PC, Prefetch_Addr
    LDR     PC, Abort_Addr
    .word   0                   @ Reserved vector
    LDR     PC, IRQ_Addr
    LDR     PC, FIQ_Addr

Reset_Addr:      .word Reset_Go
Undefined_Addr:  .word Undefined_Handler
SWI_Addr:        .word SWI_Handler
Prefetch_Addr:   .word Prefetch_Handler
Abort_Addr:      .word Abort_Handler
                 .word 0
IRQ_Addr:        .word IRQ_Handler
FIQ_Addr:        .word FIQ_Handler

@ Exception Handlers (dummy - infinite loop)
Undefined_Handler:
    B       Undefined_Handler

SWI_Handler:
    MOV     r0, #0
    MOVS    pc, lr

Prefetch_Handler:
    B       Prefetch_Handler

Abort_Handler:
    B       Abort_Handler

IRQ_Handler:
    B       IRQ_Handler

FIQ_Handler:
    B       FIQ_Handler


.section .init, "ax"
.global Reset_Go

@ Mode bits and interrupt flag (I&F) defines
.equ USR_MODE,  0x10
.equ FIQ_MODE,  0x11
.equ IRQ_MODE,  0x12
.equ SVC_MODE,  0x13
.equ ABT_MODE,  0x17
.equ UDF_MODE,  0x1B
.equ SYS_MODE,  0x1F

.equ I_BIT,     0x80
.equ F_BIT,     0x40

@ RAM limit for N32903 (8MB at 0x0)
.equ RAM_Limit, 0x800000

.equ USR_Stack,     RAM_Limit
.equ SVC_Stack,     (USR_Stack - 1024)
.equ FIQ_Stack,     (SVC_Stack - 256*1024)
.equ IRQ_Stack,     (FIQ_Stack - 1024)
.equ Abort_Stack,   (IRQ_Stack - 1024)
.equ UND_Stack,     (Abort_Stack - 1024)

Reset_Go:
    @--------------------------------
    @ Initial Stack Pointer register
    @--------------------------------
    MSR     CPSR_c, #UDF_MODE | I_BIT | F_BIT
    LDR     SP, =UND_Stack

    MSR     CPSR_c, #ABT_MODE | I_BIT | F_BIT
    LDR     SP, =Abort_Stack

    MSR     CPSR_c, #IRQ_MODE | I_BIT | F_BIT
    LDR     SP, =IRQ_Stack

    MSR     CPSR_c, #FIQ_MODE | I_BIT | F_BIT
    LDR     SP, =FIQ_Stack

    MSR     CPSR_c, #SYS_MODE | I_BIT | F_BIT
    LDR     SP, =USR_Stack

    MSR     CPSR_c, #SVC_MODE | I_BIT | F_BIT
    LDR     SP, =SVC_Stack

    @------------------------------------------------------
    @ Set the normal exception vector of CP15 control bit
    @------------------------------------------------------
    MRC     p15, 0, r0, c1, c0        @ r0 := cp15 register 1
    BIC     r0, r0, #0x2000           @ Clear bit13 in r0
    MCR     p15, 0, r0, c1, c0        @ cp15 register 1 := r0

    @--------------------------------------------------
    @ Zero .bss (the IBR only copies the loaded image)
    @--------------------------------------------------
    LDR     r0, =__bss_start__
    LDR     r1, =__bss_end__
    MOV     r2, #0
1:  CMP     r0, r1
    STRLO   r2, [r0], #4
    BLO     1b

    @-----------------------------
    @ Enter the Rust code
    @-----------------------------
    BL      nand_loader_main

    @ Should never return - loop forever
    B       .

.end
.size Reset_Go, .-Reset_Go
