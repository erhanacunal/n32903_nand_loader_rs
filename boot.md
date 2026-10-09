# N32903 NAND Loader — Complete Architecture Walkthrough

## 1. Overview

This bootloader is the **first-stage loader** for the Nuvoton N32903 (ARM926EJ-S) SoC. Its sole purpose: read the real application firmware from NAND Flash, copy it to SDRAM, and jump to it.

```
┌──────────────────────────────────────────────────────────────────────┐
│                        Power-On / Reset                              │
│                          │                                           │
│                          ▼                                           │
│              CPU fetches first instruction at 0x00000000             │
│         (Internal BOOT ROM maps first NAND page here)                │
│                          │                                           │
│                          ▼                                           │
│  ┌──────────────────────────────────────────────┐                    │
│  │         Startup Assembly (asm/startup.s)      │                    │
│  │  1. Set up stack for all ARM modes           │                    │
│  │  2. Configure CP15 exception vector table    │                    │
│  │  3. Branch to nand_loader_main()             │                    │
│  └──────────────────────────────────────────────┘                    │
│                          │                                           │
│                          ▼                                           │
│  ┌──────────────────────────────────────────────┐                    │
│  │          main() / nand_loader_main()         │                    │
│  │  1. Clear SRAM boot header                   │                    │
│  │  2. Initialize SPU DAC (pop suppression)     │                    │
│  │  3. Configure PLL clock (UPLL 96MHz)        │                    │
│  │  4. Configure RTC hardware power-off         │                    │
│  │  5. Initialize UART debug serial             │                    │
│  │  6. Initialize DMAC + NAND controller        │                    │
│  │  7. Read Image Header from NAND              │                    │
│  │  8. Parse Image List → Logo → Execute        │                    │
│  │  9. Jump to application firmware             │                    │
│  └──────────────────────────────────────────────┘                    │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 2. Assembly Startup Stage (`asm/startup.s`)

This is the first code the CPU executes, adapted from the C version's `vectors.s` + `wb_init.s`:

```asm
; Exception Vector Table (must be at 0x00000000)
Vector_Table:
    LDR  PC, Reset_Addr        ; Reset → Reset_Go
    LDR  PC, Undefined_Addr    ; Undefined instruction
    LDR  PC, SWI_Addr          ; Software interrupt
    LDR  PC, Prefetch_Addr     ; Prefetch abort
    LDR  PC, Abort_Addr        ; Data abort
    NOP                        ; Reserved
    LDR  PC, IRQ_Addr          ; IRQ interrupt
    LDR  PC, FIQ_Addr          ; FIQ interrupt
```

**Reset_Go** does three things:

```asm
Reset_Go:
    ; 1. Set up stack pointers for every ARM CPU mode
    MSR  CPSR_c, #UDF_MODE   ; Undefined mode
    LDR  SP, =UND_Stack
    ...same for ABT_MODE, IRQ_MODE, FIQ_MODE, SYS_MODE...

    MSR  CPSR_c, #SVC_MODE   ; Supervisor mode (where C/Rust code runs)
    LDR  SP, =SVC_Stack

    ; 2. Configure CP15 coprocessor: clear bit13
    ;    → exception vectors at 0x00000000 (low vectors)
    MRC  p15, 0, r0, c1, c0
    BIC  r0, r0, #0x2000
    MCR  p15, 0, r0, c1, c0

    ; 3. Enter Rust code
    BL   nand_loader_main      ; never returns
```

**Why set up stacks for all modes?** ARM has separate `sp` registers per exception mode. If an exception fires in a mode with no stack set up, the CPU corrupts memory.

**Why clear CP15 bit13?** At power-on, the N32903 BOOT ROM sets bit13, pointing vectors to `0xFFFF0000` (high vectors). Clearing it restores `0x00000000` (low vectors). This ensures the loaded application sees the standard ARM vector layout.

---

## 3. main() Execution Flow

### 3.1 Hardware Initialization Sequence

```
main()
  │
  ├─ (1) Clear SRAM header (0xFF000000 = 0)
  │     Prevents stale Boot Code Header from causing boot failures
  │
  ├─ (2) spuDacOn(2)
  │     Initialize audio DAC to prevent speaker "pop" at power-on:
  │     - Enable SPU clock
  │     - Reset SPU controller
  │     - Gradually power up DAC stages (P7→P6→P1-4→P5→P0)
  │
  ├─ (3) initClock()
  │     The most complex hardware config step:
  │
  │     3a. Set DDR timing parameters
  │         REG_DQSODS  = 0x1010       (DQS output delay)
  │         REG_CKDQSDS = 0x00888800   (clock & DQS skew)
  │         REG_SDREF   = 0x805A/0x80C0 (refresh rate, 12KHz/27KHz xtal)
  │         REG_SDTIME  = 0x094E7425   (DDR grade-6 timings)
  │         REG_SDMR    = 0x22         (CAS Latency = 2)
  │
  │     3b. sysSetSystemClock(UPLL, 96MHz)
  │         - Compute PLL divider register value (NF/NR/NO)
  │         - Copy sysClockSwitch() code to SRAM for execution
  │           (DDR enters self-refresh during clock switch — code MUST run from SRAM)
  │         - DDR self-refresh → switch PLL → configure dividers →
  │           exit self-refresh → new frequency active
  │
  │     Final clock tree: UPLL 96MHz → SYS 96MHz → CPU 96MHz
  │                                       → HCLK 48MHz (DRAM)
  │                                       → APB 24MHz (peripherals)
  │
  ├─ (4) RTC configuration
  │     REG_AER   = 0xA965   (unlock RTC access)
  │     REG_PWRON = 0x60005  (hold power key 6 sec → power off)
  │     REG_RIIR  = 0x4      (clear RTC interrupt)
  │
  ├─ (5) UART initialization
  │     Baud rate 115200, 8N1
  │     Computation: DIV = 12MHz / 115200 - 2 = 102
  │     REG_UART_BAUD = (0x30 << 24) | 102
  │
  ├─ (6) NAND controller initialization
  │     fmiInitDevice():
  │       - Enable NAND & SIC clocks (AHBCLK)
  │       - Initialize DMAC (DMA Controller)
  │       - Reset FMI (Flash Memory Interface)
  │
  │     sicSMInit():
  │       - Configure NAND pin mux (GPD/GPF registers)
  │       - Reset SM controller
  │       - Configure ECC (BCH T4) + redundant-area auto read/write
  │       - Set NAND timing (REG_SMTCR = 0x3050B)
  │       - Read NAND ID to auto-detect chip model & parameters
  └
```

### 3.2 The Clock Switch "Dangerous Dance"

This deserves a dedicated explanation — it's the most critical and risky step:

```
Problem: Switching the PLL requires stable clock for DRAM refresh.
         But during the switch, the clock temporarily disappears or changes,
         which could corrupt DRAM data.

Solution: Execute a "DDR self-refresh + SRAM execution" protocol

1. Copy sysClockSwitch() function code from DDR to SRAM
   (SRAM doesn't need refresh — survives clock loss)

2. Put DDR into Self-Refresh mode
   REG_SDCMD |= AUTOEXSELFREF | REF_CMD
   DRAM now maintains its own data, independent of the controller

3. Write new PLL register value
   REG_UPLLCON = new_value

4. Wait for PLL lock (~1ms delay)

5. Switch clock source to new PLL
   REG_CLKDIV0 = (UPLL << 3) | divider

6. Configure CPU/HCLK/APB dividers
   REG_CLKDIV4 = (CPU_N | APB_N << 8)

7. Exit DDR self-refresh
   REG_SDCMD &= ~REF_CMD

8. Restore original SRAM data
   (since the application may use that SRAM area later)
```

---

## 4. NAND Flash Driver (`nand_drv.rs`)

### 4.1 FMI (Flash Memory Interface) Architecture

```
   CPU / ARM9
      │
      ▼
   DMAC ──── auto-transfer data ────► DDR / SDRAM
      │
      │ REG_DMACSAR (target address)
      │ REG_SMCSR   (control)
      ▼
   FMI ────────────────────────────► NAND Flash
      │
      ├─ SMCMD  (Command register, 0xB10068B0)
      ├─ SMADDR (Address register, 0xB10068B4)
      ├─ SMDATA (Data register,    0xB10068B8)
      ├─ SMCSR  (Control/Status,   0xB10068A0)
      ├─ SMISR  (Interrupt Status, 0xB10068AC)
      └─ SMRA_0..54 (Redundant Area, 0xB1006A00+)
```

### 4.2 NAND Chip Detection (`fmi_sm_read_id`)

```
1. Send Reset command (0xFF)
   ↓
2. Send Read ID command (0x90) + address 0x00
   ↓
3. Read 5-byte ID: [Maker][Device][...]
   ↓
4. Look up parameters by Device ID:
   ├─ 0x79/0x76/0x75/0x73 → 512B page (16~128MB)
   ├─ 0xF1/0xD1/0xDA/0xDC → 2KB page  (128~512MB)
   ├─ 0xD3/0xD5           → 2KB/4KB   (1~2GB)
   └─ Special case detection:
       ├─ Micron MT29F4G08ABAEA     → 4KB page, ECC12
       ├─ Toshiba TC58NVG2S0HTA00   → 4KB page, ECC8
       ├─ Samsung K9WAG08U1D        → 2KB page
       └─ MXIC series → force-enable ECC
```

### 4.3 Page Read Flow (2KB page example)

```
sicSMpread(chip, block, page, buffer)
  │
  ├─ sicSMselect(chip) → select NAND chip select line
  │
  ├─ For Block 0-3 (boot blocks), downgrade to ECC4
  │   (BOOT ROM writes these blocks with ECC4 only)
  │
  ├─ Step 1: fmiSM_Read_RA() → read Redundant Area (64 bytes)
  │   │ CMD: 0x00 → ADDR: 2048 (column) → ADDR: page → CMD: 0x30
  │   │ Loop 64 times, reading SMDATA into SMRA registers
  │   ▼
  │
  ├─ Step 2: fmiSM_Read_2K() → DMAC transfer 2KB page data
  │   │ CMD: 0x00 → ADDR: 0, 0, page → CMD: 0x30
  │   │ Wait for R/B# signal (Ready/Busy)
  │   │ REG_SMCSR |= SMCR_DRD_EN → DMAC starts transfer
  │   │
  │   │ Real-time ECC correction during transfer:
  │   │ while DMAC not done:
  │   │   if ECC_FIELD_IF:          ← error detected
  │   │     read SM_ECC_ST0         ← error status register
  │   │     for each field (1..4):
  │   │       if correctable:
  │   │         count = status >> 2
  │   │         fmiSM_CorrectData_BCH(field, count, buffer)
  │   │           ├─ Read error data  from REG_BCH_ECC_DATAx
  │   │           ├─ Read error addr  from REG_BCH_ECC_ADDRx
  │   │           ├─ Determine: data area vs redundant area
  │   │           └─ buffer[offset] ^= error_data   (XOR flip bits)
  │   │       elif uncorrectable:
  │   │         return -1            ← fatal error
  │   │   if DMA_IF: break           ← transfer complete
  │   ▼
  │
  └─ Restore ECC8 (if this was Block 0-3)
```

### 4.4 BCH ECC Correction Principle

The N32903 hardware BCH engine computes ECC on-the-fly for each page read, compares it against the stored ECC, and reports the exact bit position and value of each error. Software only needs to flip those bits:

```
Hardware provides:
  - REG_BCH_ECC_DATAx: error data (which bits to flip)
  - REG_BCH_ECC_ADDRx: error address (which byte offset)

Software correction:
  buffer[addr] ^= data;     // XOR to flip the erroneous bits

Address regions:
  - addr < 512:          main data area → buffer[addr] ^= data
  - 512 ≤ addr < 515:    redundant area (front half) → SMRA[addr-512] ^= data
  - addr ≥ 515:          redundant area (back half) → reverse-mapped SMRA[offset] ^= data
```

Supported ECC strengths: T4 (4 bits/512B), T8 (8 bits/512B), T12 (12 bits/512B), T15 (15 bits/512B).

---

## 5. Bad Block Management (`check_bad_block_mark`)

NAND Flash ships from the factory with bad blocks. The loader must detect and skip them:

```
check_bad_block_mark(blockNo)
  │
  ├─ Block 0: always good (special rule)
  │
  ├─ Step 1: Read first page's redundant area → check Bad Block Marker
  │
  │   For 512B-page NAND:  check redundant Byte 1 and Byte 6
  │   For 2KB/4KB-page NAND: check redundant Byte 1
  │
  │   if marker == 0xFF (i.e. "not marked bad"):
  │     ├─ SLC NAND: check 2nd page to confirm
  │     └─ MLC NAND: check LAST page to confirm
  │
  │   if confirmation page marker != 0xFF: → BAD  (return 1)
  │   else:                                 → GOOD (return 0)
  │
  └─ if marker != 0xFF: → BAD
```

**Why check two pages?** Manufacturers mark bad blocks at a specific byte in either the 1st or 2nd page (SLC), or the last page (MLC). If the first page was accidentally erased, the second page still carries the factory mark.

---

## 6. Image Format and Parsing

### 6.1 Image Header Layout on NAND

Firmware images aren't stored as a raw binary. An **Image Header** lives at the **second-to-last page** of each block:

```
Second-to-last page of Block (page = pagePerBlock - 2):
┌─────────────────────────────────────────────────────────────────┐
│  Offset   │  Field                 │  Size    │  Value          │
├───────────┼────────────────────────┼──────────┼─────────────────┤
│  0x00     │  ActionFlag            │  4 bytes │  0x574255AA     │  ← Magic #1
│  0x04     │  ImageCount            │  4 bytes │  N (total images)
│  0x08     │  ImageNo               │  4 bytes │  0              │
│  0x0C     │  ImageName[16]         │ 16 bytes │  "..."          │
│  0x1C     │  ActionFlag2           │  4 bytes │  0x57425963     │  ← Magic #2
├───────────┼────────────────────────┼──────────┼─────────────────┤
│  Below: N Image descriptors, each 12 × u32 (48 bytes)            │
├───────────┼────────────────────────┼──────────┼─────────────────┤
│  +0       │  ImageType (high 16)   │  4 bytes │  1 = Execute    │
│           │  ImageNo   (low 16)    │          │  4 = Logo       │
│  +4       │  StartBlock (low 16)   │  4 bytes │  start block #  │
│           │  EndBlock   (high 16)  │          │  end block #    │
│  +8       │  ExecuteAddr           │  4 bytes │  load-to address │
│  +12      │  FileLen               │  4 bytes │  size in bytes  │
├───────────┼────────────────────────┼──────────┼─────────────────┤
│  ... next image descriptor ...                                    │
└─────────────────────────────────────────────────────────────────┘
```

### 6.2 Image Discovery Flow

```
search_image()
  │
  ├─ Scan second-to-last page of Block 0, 1, 2, 3
  │   (Why 4 blocks? Redundancy — if one is bad, others are backups)
  │
  ├─ Validate Magic Numbers:
  │   pImageList[0] == 0x574255AA  ("WBUª")
  │   pImageList[3] == 0x57425963  ("WBYc")
  │
  ├─ Iterate Image list:
  │   for each image descriptor:
  │     imageType = (descriptor[0] >> 16) & 0xFFFF
  │
  │     ① Logo image (type == 4)
  │       - Load to executeAddr, but DO NOT jump
  │       - IsExecute = FALSE → just copy data
  │       - Purpose: display a boot splash screen
  │
  │     ② Execute image (type == 1)
  │       - Load to executeAddr
  │       - IsExecute = TRUE → after loading, jump to it
  └
```

### 6.3 MoveData — The Core Data-Copying Routine

```
MoveData(image, isExecute)
  │
  │ image = { startBlock, endBlock, executeAddr, fileLen }
  │
  ├─ Compute:
  │   pageCount  = ceil(fileLen / pageSize)
  │   blockCount = pageCount / pagesPerBlock
  │   remainder  = pageCount % pagesPerBlock
  │
  ├─ Read full blocks (every page within each block):
  │   for each block needed:
  │     if block is bad → skip to next block  ← auto bad-block skipping!
  │     else:
  │       for each page in block:
  │         sicSMpread(0, block, page, addr)
  │         addr += pageSize
  │
  ├─ Read remaining pages (less than one full block):
  │   find first good block → read only the needed page count
  │
  └─ if isExecute:
      ① Release NAND GPIO pins (restore GPIO defaults)
      ② Release SD card GPIO pins
      ③ fw_func = (void(*)(void))executeAddr
      ④ fw_func()  ← never returns!
```

**Key design: automatic bad-block skipping**

```
Physical blocks:  [0] [1] [2] [3] [BAD] [5] [6] ...
                  读  读  读  读  skip  读  读
                                     ▲
                                 curBlock++ and continue
```

It doesn't use absolute block numbers — it counts *good blocks consumed*. If 5 blocks of data are needed but block 4 is bad, it reads blocks 0,1,2,3,5.

---

## 7. Design Rationale

| Problem | Solution |
|---|---|
| BOOT ROM uses only ECC4 for Block 0-3 | Temporarily switch to ECC4 when reading these blocks, restore afterward |
| NAND ships with factory bad blocks | `check_bad_block_mark()` tests every block before reading |
| Many different NAND chip models exist | `fmiSM_ReadID()` auto-detects 20+ NAND chip types |
| Splash logo must display before main app | Load Logo image first (no jump), then Execute image (with jump) |
| Switching PLL corrupts DDR contents | DDR self-refresh before switch, code runs from SRAM during switch |
| Hardware power-off needs RTC support | Configure RTC mask, hold power key 6 seconds triggers shutdown |
| DMA consistency with CPU caches | `address \| 0x80000000` sets non-cacheable bit on image_buffer |

---

## 8. Memory Layout (N32903, 8MB SDRAM)

```
0x0000_0000 ┌──────────────────────┐
            │  Exception Vectors    │  ← vectors.s (internal SRAM mapping)
            │  (BOOT ROM mapped)    │
0x0070_0000 ├──────────────────────┤
            │  .text (code)         │  ← main(), NAND driver, clock, UART
            │  .rodata (constants)  │
            │  .data (initialized)  │
            │  .bss  (zero-init)    │
            ├──────────────────────┤
            │  image_buffer         │  4096 bytes, 32-byte aligned
            │  (4KB work buffer)    │
            ├──────────────────────┤
            │  heap                 │  (effectively unused)
            │  ...                  │
0x0080_0000 ├──────────────────────┤
            │  Stack (SVC mode)     │  ← grows downward
            │  FIQ/IRQ/ABT stacks  │
            │  User/System stack    │
0x0??_????? ├──────────────────────┤
            │  Application firmware │  ← move_data() loads here
            │  (Logo + Execute)     │
            ├──────────────────────┤
0x0???_???? │  Free SDRAM           │
            └──────────────────────┘
0x8000_0000
```

The Image Header sits at the second-to-last page of NAND Block 0. When the loader finds a valid header, it follows the descriptors to copy firmware from NAND to SDRAM at the specified `executeAddr`, then calls `fw_func()` — at that point the NAND Loader's job is done and the application takes over the system.

---

## Boot Menu and UART Update

After NAND init the loader prints `Press any key to enter menu : 3` and counts
down for 3 seconds. With no input it boots the application. Any key opens the
`n32903> ` command line:

| Command    | Action                                                          |
|------------|-----------------------------------------------------------------|
| `writeapp` | Receive an application image and write it from block 4 on       |
| `writeldr` | Receive a loader image (with IBR header) and write blocks 0-3   |
| `boot`     | Load and run the application                                    |

If the application can't be loaded, the loader opens this menu instead of
halting.

Images are sent with `flash_tool.py` (needs `pip install pyserial`):

```
python flash_tool.py COM5 writeapp firmware.bin --boot
python flash_tool.py COM5 writeldr nandloader.bin
python flash_tool.py COM5 boot --monitor
```

The tool interrupts the countdown itself; reset the board when it asks. The
whole image is received into SDRAM and CRC-checked before any block is
erased. Every programmed page is read back and compared. Bad blocks are
skipped, and an application block that fails is marked bad. `writeldr`
stops at the first failing boot block so the remaining copies keep the old
loader. The wire protocol is documented at the top of `src/update.rs`.
