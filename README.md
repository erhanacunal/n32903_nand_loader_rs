# N32903 NAND Loader (Rust)

A Rust port of the Nuvoton N32903 (W55FA93, ARM926EJ-S) NandLoader. The boot
ROM (IBR) copies it from NAND blocks 0-3 to SDRAM at `0x700000` and runs it.
The loader brings up the clocks, UART and NAND. It then copies the application
from block 4 to SDRAM `0x0` and jumps to it.

During a 3 second countdown on the UART, a key press opens a small command
line. From there the application or the loader itself can be updated over the
serial port.

## Features

- System clock at 200 MHz: UPLL 200, CPU 200, HCLK 100 (DDR), APB 50, from the
  12 MHz crystal. The clock switch runs from SRAM and does not touch SDRAM
  while it is in self-refresh.
- NAND driver (FMI/SIC) with BCH ECC and bad block skipping.
- Console on UART1, 115200 8N1. The board gates TX through a transistor, so
  the loader enables it on GPA7.
- `writeapp` / `writeldr` update over UART. The whole image is received and
  CRC-checked in SDRAM before any block is erased, and every page is read back
  after programming.
- If the application can't be loaded, the loader opens the menu instead of
  halting.

## NAND layout

| Blocks | Contents                                                          |
|--------|-------------------------------------------------------------------|
| 0-3    | Loader with its 32-byte IBR boot header (four copies, with the IBR spare marks) |
| 4+     | Application: `size` (u32 LE) followed by the raw binary           |

The application is loaded to `0x0` and must fit below the loader at
`0x700000`.

## Requirements

- Rust nightly with `rust-src` (selected by `rust-toolchain.toml`; the build
  uses `build-std` and a custom target, `arm926ej-s.json`)
- ARM GNU toolchain (`arm-none-eabi-gcc`, `-ld`, `-objcopy`). `build.rs`
  assembles `asm/startup.s` with `arm-none-eabi-gcc`, which is also the linker.
- Python 3, plus `pyserial` for `flash_tool.py` (`pip install pyserial`)
- Optional: OpenOCD and a JTAG adapter, to run the loader without flashing it

## Building

```powershell
.\build.ps1           # release build
.\build.ps1 -Debug    # debug build
.\build.ps1 -Clean    # cargo clean first
```

`build.ps1` expects the ARM toolchain at `D:\armeabi_none_toolchain\bin`;
edit the paths at the top of the script for another location. It produces:

| File | Description |
|------|-------------|
| `target/arm926ej-s/release/n32903_nand_loader` | ELF |
| `target/arm926ej-s/release/n32903_nand_loader.bin` | Raw binary, linked at `0x700000` (for JTAG) |
| `nandloader.bin` | Raw binary with the IBR boot header (for `writeldr`) |

The header is added by `prepend_header.py`:

```
python prepend_header.py <raw.bin> -o nandloader.bin --exec-addr 0x700000
```

Without the script, the equivalent cargo command is
`cargo build --release -Zjson-target-spec`.

## Boot menu

At startup the loader prints the chip, clock and NAND information, then
`Press any key to enter menu : 3`. With no input it boots the application.
Any key opens the `n32903> ` prompt:

| Command    | Action                                                        |
|------------|---------------------------------------------------------------|
| `writeapp` | Receive an application image and write it from block 4 on     |
| `writeldr` | Receive a loader image (with IBR header) and write blocks 0-3 |
| `boot`     | Load and run the application                                  |
| `help`     | List the commands                                             |

## Flashing over UART

`flash_tool.py` interrupts the countdown (reset the board when it asks) or
reuses an already open menu, then runs one command:

```
python flash_tool.py COM5 writeapp firmware.bin --boot
python flash_tool.py COM5 writeldr nandloader.bin
python flash_tool.py COM5 boot --monitor
```

Options: `-b/--baud` (default 115200), `-w/--wait` (seconds to wait for the
loader, default 30), `--boot` (boot after writing), `-m/--monitor` (keep
printing serial output).

Bad blocks are skipped, and an application block that fails is marked bad.
`writeldr` stops at the first failing boot block, so the remaining copies keep
the old loader. The wire protocol is documented at the top of `src/update.rs`.

## Running over JTAG

`jtag_run_loader.tcl` loads the raw binary (no IBR header) to `0x700000` and
starts it without a reset. A reset disables JTAG on the Nextion board, so
power up with the JTAG switch pressed, then:

```
openocd -f <interface/target cfg> -f jtag_run_loader.tcl \
        -c "init; run_loader target/arm926ej-s/release/n32903_nand_loader.bin"
```

`load_loader` does the same but leaves the CPU halted at `0x700000`, so
breakpoints can be set before `resume`. Once the loader is running from RAM,
flash it with `flash_tool.py ... writeldr nandloader.bin`.

## Source layout

| Path | Description |
|------|-------------|
| `asm/startup.s` | Reset entry, mode stacks, vector setup |
| `src/main.rs` | Hardware init sequence, application load and jump |
| `src/clock.rs` | PLL and clock dividers, clock switch run from SRAM |
| `src/nand_drv.rs` | FMI/SIC NAND driver, BCH ECC, bad block checks |
| `src/uart.rs` | UART1 console and `printf`-style output |
| `src/timer.rs` | TIMER0 millisecond timebase for countdowns and timeouts |
| `src/menu.rs` | Autoboot countdown and command line |
| `src/update.rs` | `writeapp` / `writeldr` receive, program and verify |
| `src/crc32.rs` | CRC-32 for the update protocol |
| `src/depop.rs` | SPU DAC init to suppress the power-on pop |
| `src/registers.rs` | Register addresses |
| `linker.ld` | Memory map, everything linked at `0x700000` |
| `boot.md` | Detailed walkthrough of the original loader's design |

`boot.md` describes the original C loader's architecture (startup, clock
switch, NAND driver, ECC). Some of it, such as the image table and the
96 MHz clock, no longer matches this port; the README and the source are
current.
