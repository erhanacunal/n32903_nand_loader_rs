//! N32903 NAND Boot Loader (simplified — no image table, no logo)
//!
//! NAND layout:
//!   Block 0-3  → loader itself (written by BOOT ROM / programming tool)
//!   Block 4+   → application firmware
//!
//! Block 4 format:
//!   Byte 0..3   → firmware size in bytes (little-endian u32)
//!   Byte 4..    → firmware binary
//!
//! The loader reads the 4-byte header, copies that many bytes to SDRAM,
//! then jumps to the execute address.
//!
//! At startup a 3 second countdown runs; a key press on the UART opens a
//! command line (writeapp / writeldr / boot) for updating over UART.

#![no_std]
#![no_main]

/// Busy-wait `n` iterations.  A plain Rust countdown loop has no visible
/// effect and is optimised away; this one can't be (same idea as the
/// `asm volatile` loops in wb_config.c).  A macro, not a function: code
/// running from the SRAM copy must not call out.
macro_rules! spin {
    ($n:expr) => {{
        let n: u32 = $n;
        if n != 0 {
            core::arch::asm!(
                "1:",
                "subs {0}, {0}, #1",
                "bne 1b",
                inout(reg) n => _,
                options(nostack),
            )
        }
    }};
}

mod registers;
mod uart;
mod clock;
mod depop;
mod nand_drv;
mod timer;
mod crc32;
mod update;
mod menu;

use core::panic::PanicInfo;
use uart::UartArg;

// ============================================================================
// Static NAND layout — only the starting block and execute address are fixed
// ============================================================================

/// NAND block where the application firmware header begins.
pub const FW_START_BLOCK: u32 = 4;

/// SDRAM address to load the firmware into.
const FW_EXECUTE_ADDR: u32 = 0x0000_0000;

/// Date code shown at boot for identification.
const DATE_CODE: &str = "20201214";

/// Page-sized buffer for reading the firmware header.
/// Static (not stack) because the SVC stack is only 1KB but pages can be 4KB.
static mut HEADER_PAGE: [u8; 4096] = [0u8; 4096];

// ============================================================================
// Core: read firmware size from NAND, copy to SDRAM, jump
// ============================================================================

/// Load the firmware and jump to it.  Returns only if it cannot be loaded.
unsafe fn load_and_jump(start_block: u32, execute_addr: u32) {
    use crate::registers::*;

    let sm0 = nand_drv::get_sm0();

    // --- 1. Read first page of the first good block to get the size header ---
    let mut start_block = start_block;
    while nand_drv::check_bad_block_mark(start_block) != nand_drv::SUCCESSFUL {
        start_block += 1;
        if start_block >= sm0.block_per_flash {
            uart::uart_puts("ERROR: no good block for firmware\n");
            return;
        }
    }
    let header = &mut *core::ptr::addr_of_mut!(HEADER_PAGE);
    nand_drv::sic_sm_pread(0, start_block as i32, 0, header.as_mut_ptr());

    let file_len = u32::from_le_bytes([
        header[0], header[1], header[2], header[3],
    ]);

    uart::uart_printf(
        "Firmware header: size=0x%x (%d bytes)\n",
        &[UartArg::U32(file_len), UartArg::U32(file_len)],
    );

    if file_len == 0 || file_len == 0xFFFF_FFFF {
        // Sanity check — don't try to load garbage
        uart::uart_puts("ERROR: invalid firmware size\n");
        return;
    }

    if file_len > update::max_app_size() {
        uart::uart_puts("ERROR: firmware would overwrite the loader\n");
        return;
    }

    // --- 2. Copy firmware from NAND to SDRAM ---
    //
    // NAND layout in Block 4:
    //   Page 0: [4-byte size LE][firmware bytes...]
    //   Page 1+: [firmware bytes...]
    //
    // Count pages by NAND footprint (4-byte header + file_len), stream
    // pages into SDRAM (page 0 with a 4-byte skip), skip bad blocks.
    let page_size = sm0.page_size;
    let pages_per_block = sm0.page_per_block;

    let total_nand_bytes = 4u32.saturating_add(file_len);
    let total_pages = (total_nand_bytes + page_size - 1) / page_size;

    let mut addr = execute_addr;
    let mut pages_done: u32 = 0;
    let mut cur_block = start_block;

    loop {
        if pages_done >= total_pages {
            break;
        }
        if cur_block >= sm0.block_per_flash {
            uart::uart_puts("ERROR: firmware runs past end of NAND\n");
            return;
        }
        if nand_drv::check_bad_block_mark(cur_block) != nand_drv::SUCCESSFUL {
            cur_block += 1;
            continue;
        }

        let pages_this_block = (total_pages - pages_done).min(pages_per_block);
        for p in 0..pages_this_block {
            let abs_page = pages_done + p;

            if abs_page == 0 {
                // First page: already in HEADER_PAGE.  Copy from offset 4.
                let copy_len = (page_size as usize - 4).min(file_len as usize);
                copy_to_phys(addr, header.as_ptr().add(4), copy_len);
                addr += copy_len as u32;
            } else {
                // Normal page: DMA from NAND directly to SDRAM.
                nand_drv::sic_sm_pread(0, cur_block as i32, p as i32, addr as *mut u8);
                addr += page_size;
            }
        }
        pages_done += pages_this_block;
        cur_block += 1;
    }

    // --- 7. Release hardware, jump ---
    reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) & !0x0003_FC00);
    reg_write32(REG_GPEFUN, reg_read32(REG_GPEFUN) & !0x00FF_0000);
    reg_write32(REG_GPEFUN, reg_read32(REG_GPEFUN) & !0x0000_FFF0);

    timer::deinit();
    uart::uart_puts("Jumping to firmware...\n");
    uart::flush();

    // Jump in asm: execute_addr is 0, and Rust treats a call through a
    // null function pointer as UB (it compiled to an undefined instruction).
    core::arch::asm!("bx {0}", in(reg) execute_addr, options(noreturn));
}

/// Byte copy to a physical address.  Unlike copy_nonoverlapping this is
/// valid for address 0, where the firmware is loaded.
unsafe fn copy_to_phys(dst: u32, src: *const u8, len: usize) {
    if len == 0 {
        return;
    }
    core::arch::asm!(
        "1:",
        "ldrb {t}, [{s}], #1",
        "strb {t}, [{d}], #1",
        "subs {n}, {n}, #1",
        "bne 1b",
        s = inout(reg) src => _,
        d = inout(reg) dst => _,
        n = inout(reg) len => _,
        t = out(reg) _,
        options(nostack),
    );
}

// ============================================================================
// Main entry point (called from assembly startup)
// ============================================================================

#[no_mangle]
pub unsafe extern "C" fn nand_loader_main() -> i32 {
    use crate::registers::*;

    // -- hardware init --------------------------------------------------------
    reg_write32(0xFF00_0000, 0);          // clear SRAM boot header
    depop::spu_dac_on(2);                 // audio pop suppression
    clock::init_clock();                  // DDR init + PLL 96 MHz
    // RTC power-key setup.  AER only opens when the RTC is initialised and
    // its 32 kHz clock runs; if it doesn't, skip this instead of hanging.
    reg_write32(AER, 0x0000_A965);       // unlock RTC
    let mut rtc_polls: u32 = 0;
    while reg_read32(AER) & 0x10000 == 0 && rtc_polls < 1_000_000 {
        rtc_polls += 1;
    }
    let rtc_ok = reg_read32(AER) & 0x10000 != 0;
    if rtc_ok {
        reg_write32(PWRON, 0x60005);     // power-key 6 s → shutdown
        reg_write32(RIIR, 0x4);
    }

    // -- UART ----------------------------------------------------------------
    uart::uart_init();
    uart::uart_printf("FA93 Nand Boot Loader entry (%s).\n", &[UartArg::Str(DATE_CODE)]);
    if !rtc_ok {
        uart::uart_puts("Warning: RTC not accessible, power-key setup skipped\n");
    }

    let chip_ver = clock::sys_get_chip_version();
    uart::uart_puts(if chip_ver == b'G' {
        "FA93 chip version: G\n"
    } else {
        "FA93 chip version: A ~ F\n"
    });

    let mut _src = clock::SysClkSrc::Ext;
    let mut pll = 0u32; let mut sys = 0u32; let mut cpu = 0u32;
    let mut hclk = 0u32; let mut apb = 0u32;
    clock::sys_get_system_clock(&mut _src, &mut pll, &mut sys, &mut cpu, &mut hclk, &mut apb);
    uart::uart_printf(
        "System clock=%dKHz  HCLK=%dKHz  SDTIME=0x%x\n",
        &[UartArg::U32(sys), UartArg::U32(hclk), UartArg::U32(reg_read32(REG_SDTIME))],
    );

    // -- NAND init -----------------------------------------------------------
    nand_drv::fmi_init_device();
    nand_drv::sic_sm_init();

    uart::uart_printf(
        "NAND: page=%d  pages/block=%d  blocks/flash=%d\n",
        &[
            UartArg::U32(nand_drv::get_sm0().page_size),
            UartArg::U32(nand_drv::get_sm0().page_per_block),
            UartArg::U32(nand_drv::get_sm0().block_per_flash),
        ],
    );

    // -- countdown, then boot or enter the menu ------------------------------
    if !timer::init() {
        uart::uart_puts("WARNING: TIMER0 not ticking, timeouts are approximate\n");
    }

    if !menu::autoboot_interrupted() {
        load_and_jump(FW_START_BLOCK, FW_EXECUTE_ADDR);
        uart::uart_puts("Boot failed, entering menu.\n");
    }
    loop {
        menu::run();
        load_and_jump(FW_START_BLOCK, FW_EXECUTE_ADDR);
        uart::uart_puts("Boot failed.\n");
    }
}

// ============================================================================
// Panic handler
// ============================================================================

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

// ============================================================================
// Toolchain stubs
// ============================================================================

#[no_mangle]
pub extern "C" fn _sbrk(_incr: isize) -> *mut u8 {
    core::ptr::null_mut()
}
