//! `writeapp` / `writeldr`: receive an image over UART and program NAND.
//!
//! Wire protocol (host side: flash_tool.py).  The device sends single
//! control bytes ACK (0x06) / NAK (0x15); everything else it sends is plain
//! text for the user.
//!
//!   1. Device: ACK = ready for header (NAK = refused, reason printed).
//!   2. Host:   header = "NLDR" | size u32 LE | crc32(image) u32 LE
//!   3. Device: ACK = accepted, NAK = rejected.
//!   4. Host:   chunks of `offset u32 LE | data | crc32(offset bytes + data) u32 LE`
//!              where offset is a multiple of CHUNK and data is
//!              min(CHUNK, size - offset) bytes.
//!      Device: ACK = stored, NAK = send it again.  A chunk that was
//!              already stored (its ACK got lost) is ACKed again.
//!   5. Device programs and verifies NAND, printing progress, then sends
//!      ACK = success or NAK = failure.
//!
//! The whole image is received into SDRAM and CRC-checked before any NAND
//! block is erased, so an interrupted transfer never touches the flash.

use crate::crc32::Crc32;
use crate::nand_drv;
use crate::uart::{self, UartArg};
use crate::FW_START_BLOCK;

const ACK: u8 = 0x06;
const NAK: u8 = 0x15;
const CTRL_C: u8 = 0x03;
const MAGIC: [u8; 4] = *b"NLDR";
const CHUNK: u32 = 4096;

/// How long to wait for the host to start sending the header.
const HEADER_TIMEOUT_MS: u32 = 30_000;
/// Max gap between bytes inside a frame.
const BYTE_TIMEOUT_MS: u32 = 1_000;
/// Consecutive bad/missing chunks before giving up.
const MAX_RETRIES: u32 = 10;

/// IBR boot header marker at the start of a loader image.
const BOOT_CODE_MARKER: u32 = 0x5742_5AA5;
const BOOT_HEADER_LEN: u32 = 32;
const LOADER_BLOCKS: u32 = 4;

/// Images are staged in SDRAM below the loader (which lives at
/// __image_start).  Kept off address 0 so it is never a null pointer.
const UPLOAD_BUF: u32 = 0x0000_1000;
const MAX_PAGE: u32 = 4096;

#[repr(align(4))]
struct PageBuf([u8; MAX_PAGE as usize]);

/// Read-back buffer for verification (DMA target, must be word aligned).
static mut VERIFY_PAGE: PageBuf = PageBuf([0; MAX_PAGE as usize]);

extern "C" {
    static __image_start: u8;
}

fn loader_base() -> u32 {
    core::ptr::addr_of!(__image_start) as u32
}

/// Largest application image accepted by `writeapp` (and by the boot path).
pub fn max_app_size() -> u32 {
    loader_base() - UPLOAD_BUF - 2 * MAX_PAGE
}

// ============================================================================
// Commands
// ============================================================================

pub unsafe fn cmd_writeapp() {
    let Some(size) = receive("application", UPLOAD_BUF + 4, max_app_size()) else {
        return;
    };
    finish(write_app(size));
}

pub unsafe fn cmd_writeldr() {
    let sm = nand_drv::get_sm0();
    let Some(size) = receive("loader", UPLOAD_BUF, sm.page_size * sm.page_per_block) else {
        return;
    };
    finish(write_loader(size));
}

unsafe fn finish(ok: bool) {
    if ok {
        uart::uart_puts("Update complete.\n");
        uart::put_raw(ACK);
    } else {
        uart::uart_puts("Update FAILED.\n");
        uart::put_raw(NAK);
    }
}

// ============================================================================
// Receive
// ============================================================================

/// Run protocol steps 1-4.  Returns the image size, stored at `dst`.
unsafe fn receive(what: &str, dst: u32, max: u32) -> Option<u32> {
    if nand_drv::sic_sm_is_write_protected(0) {
        uart::uart_puts("ERROR: NAND is write protected\n");
        uart::put_raw(NAK);
        return None;
    }

    uart::uart_printf(
        "Ready to receive %s image (max %d bytes). Ctrl-C to cancel.\n",
        &[UartArg::Str(what), UartArg::U32(max)],
    );
    uart::put_raw(ACK);

    // --- header ---
    let mut hdr = [0u8; 12];
    match uart::getc_timeout(HEADER_TIMEOUT_MS) {
        Some(CTRL_C) => {
            uart::uart_puts("Cancelled.\n");
            return None;
        }
        Some(c) => hdr[0] = c,
        None => {
            uart::uart_puts("Timed out waiting for image.\n");
            return None;
        }
    }
    if !read_bytes(hdr.as_mut_ptr().add(1), hdr.len() - 1) || hdr[0..4] != MAGIC {
        uart::drain_rx(100);
        uart::uart_puts("ERROR: bad header\n");
        uart::put_raw(NAK);
        return None;
    }
    let size = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]);
    let image_crc = u32::from_le_bytes([hdr[8], hdr[9], hdr[10], hdr[11]]);
    if size == 0 || size > max {
        uart::uart_printf("ERROR: size %d out of range\n", &[UartArg::U32(size)]);
        uart::put_raw(NAK);
        return None;
    }
    uart::put_raw(ACK);

    // --- chunks ---
    let mut next: u32 = 0;
    let mut retries: u32 = 0;
    while next < size {
        if receive_chunk(dst, size, &mut next) {
            retries = 0;
            uart::put_raw(ACK);
        } else {
            retries += 1;
            uart::drain_rx(50);
            uart::put_raw(NAK);
            if retries >= MAX_RETRIES {
                uart::uart_puts("\nERROR: too many transfer errors\n");
                return None;
            }
        }
    }

    let mut crc = Crc32::new();
    crc.update(core::slice::from_raw_parts(dst as *const u8, size as usize));
    if crc.finish() != image_crc {
        uart::uart_puts("ERROR: image CRC mismatch\n");
        uart::put_raw(NAK);
        return None;
    }

    uart::uart_printf("Received %d bytes, CRC OK.\n", &[UartArg::U32(size)]);
    Some(size)
}

/// Receive one chunk frame.  Accepts the expected chunk or a resend of an
/// earlier one; advances `next` only for the expected one.
unsafe fn receive_chunk(dst: u32, size: u32, next: &mut u32) -> bool {
    let mut off = [0u8; 4];
    if !read_bytes(off.as_mut_ptr(), 4) {
        return false;
    }
    let offset = u32::from_le_bytes(off);
    if offset % CHUNK != 0 || offset > *next || offset >= size {
        return false;
    }

    let len = (size - offset).min(CHUNK);
    let data = (dst + offset) as *mut u8;
    let mut crc_bytes = [0u8; 4];
    if !read_bytes(data, len as usize) || !read_bytes(crc_bytes.as_mut_ptr(), 4) {
        return false;
    }

    let mut crc = Crc32::new();
    crc.update(&off);
    crc.update(core::slice::from_raw_parts(data, len as usize));
    if crc.finish() != u32::from_le_bytes(crc_bytes) {
        return false;
    }

    if offset == *next {
        *next += len;
    }
    true
}

unsafe fn read_bytes(dst: *mut u8, len: usize) -> bool {
    for i in 0..len {
        match uart::getc_timeout(BYTE_TIMEOUT_MS) {
            Some(c) => *dst.add(i) = c,
            None => return false,
        }
    }
    true
}

// ============================================================================
// Program
// ============================================================================

/// Pad the staged image with 0xFF to a whole number of pages.
unsafe fn pad_to_pages(len: u32) -> u32 {
    let page_size = nand_drv::get_sm0().page_size;
    let pages = (len + page_size - 1) / page_size;
    core::ptr::write_bytes(
        (UPLOAD_BUF + len) as *mut u8,
        0xFF,
        (pages * page_size - len) as usize,
    );
    pages
}

/// NAND layout from FW_START_BLOCK: [size u32 LE][image], skipping bad
/// blocks — the same layout load_and_jump() reads.
unsafe fn write_app(size: u32) -> bool {
    let sm = nand_drv::get_sm0();
    core::ptr::copy_nonoverlapping(size.to_le_bytes().as_ptr(), UPLOAD_BUF as *mut u8, 4);
    let pages = pad_to_pages(4 + size);

    let mut block = FW_START_BLOCK;
    let mut done: u32 = 0;
    while done < pages {
        if block >= sm.block_per_flash {
            uart::uart_puts("ERROR: ran out of NAND blocks\n");
            return false;
        }
        if nand_drv::check_bad_block_mark(block) != nand_drv::SUCCESSFUL {
            uart::uart_printf("Block %d: bad, skipped\n", &[UartArg::U32(block)]);
            block += 1;
            continue;
        }

        let n = (pages - done).min(sm.page_per_block);
        if program_block(block, UPLOAD_BUF + done * sm.page_size, n, false) {
            done += n;
        } else {
            uart::uart_printf("Block %d: marking bad, retrying on next block\n", &[UartArg::U32(block)]);
            nand_drv::sic_sm_mark_bad_block(0, block as i32);
        }
        block += 1;
    }
    true
}

/// Write the same loader image into blocks 0-3 (the IBR's boot copies).
/// Stops at the first failure so the remaining blocks keep their old copy.
unsafe fn write_loader(size: u32) -> bool {
    let buf = UPLOAD_BUF as *const u32;
    let marker = buf.read();
    let image_size = buf.add(2).read();
    if size < BOOT_HEADER_LEN || marker != BOOT_CODE_MARKER {
        uart::uart_puts("ERROR: no IBR boot header (run prepend_header.py first)\n");
        return false;
    }
    if image_size + BOOT_HEADER_LEN != size {
        uart::uart_printf(
            "ERROR: boot header ImageSize %d does not match file (%d)\n",
            &[UartArg::U32(image_size), UartArg::U32(size - BOOT_HEADER_LEN)],
        );
        return false;
    }
    uart::uart_printf("Loader ExeAddr=0x%x\n", &[UartArg::U32(buf.add(1).read())]);

    let pages = pad_to_pages(size);
    let mut written = 0;
    for block in 0..LOADER_BLOCKS {
        if block != 0 && nand_drv::check_bad_block_mark(block) != nand_drv::SUCCESSFUL {
            uart::uart_printf("Block %d: bad, skipped\n", &[UartArg::U32(block)]);
            continue;
        }
        if !program_block(block, UPLOAD_BUF, pages, true) {
            return false;
        }
        written += 1;
    }
    written > 0
}

/// Erase `block`, program `pages` pages from `src`, read back and compare.
/// `boot` writes the IBR page sequence marks used in blocks 0-3.
unsafe fn program_block(block: u32, src: u32, pages: u32, boot: bool) -> bool {
    let sm = nand_drv::get_sm0();
    let page_size = sm.page_size;

    if nand_drv::sic_sm_block_erase(0, block as i32) != nand_drv::SUCCESSFUL {
        uart::uart_printf("Block %d: erase failed\n", &[UartArg::U32(block)]);
        return false;
    }

    for p in 0..pages {
        let data = (src + p * page_size) as *const u8;
        let spare0 = if boot {
            nand_drv::spare_boot_mark(p, pages)
        } else {
            nand_drv::SPARE_DATA
        };
        if nand_drv::sic_sm_pwrite(0, block as i32, p as i32, data, spare0) != nand_drv::SUCCESSFUL {
            uart::uart_printf("Block %d page %d: program failed\n", &[UartArg::U32(block), UartArg::U32(p)]);
            return false;
        }
    }

    let verify = &mut (&mut *core::ptr::addr_of_mut!(VERIFY_PAGE)).0[..page_size as usize];
    for p in 0..pages {
        let status = nand_drv::sic_sm_pread(0, block as i32, p as i32, verify.as_mut_ptr());
        let expected = core::slice::from_raw_parts((src + p * page_size) as *const u8, page_size as usize);
        if status != nand_drv::SUCCESSFUL || verify[..] != expected[..] {
            uart::uart_printf("Block %d page %d: verify failed\n", &[UartArg::U32(block), UartArg::U32(p)]);
            return false;
        }
    }

    uart::uart_printf("Block %d: %d pages OK\n", &[UartArg::U32(block), UartArg::U32(pages)]);
    true
}
