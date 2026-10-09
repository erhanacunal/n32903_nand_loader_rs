//! UART driver for N32903 debug output
//! Ported from wb_uart.c

use crate::registers::*;
use crate::timer;

/// UART baud rate parameters
pub const WB_DATA_BITS_8: u32 = 0x03;
pub const WB_STOP_BITS_1: u32 = 0x00;
pub const WB_PARITY_NONE: u32 = 0x00;
const LEVEL_1_BYTE: u32 = 0x00;
const EXTERNAL_CRYSTAL_CLOCK: u32 = 12_000_000;

/// GPA7 enables the board's TX transistor
const TX_ENABLE_PIN: u32 = 1 << 7;
/// Level of GPA7 that lets TX through
const TX_ENABLE_LEVEL_HIGH: bool = true;

/// UART initialization status
static mut UART_INITIALIZED: bool = false;

/// UART port offset (0 for UART0, 0x100 for UART1)
static mut UART_PORT: u32 = 0x100;

/// Initialize UART hardware (UART1 by default at 115200 baud)
pub unsafe fn uart_init() {
    if UART_INITIALIZED {
        return;
    }
    UART_PORT = 0x100; // Use UART1

    // Enable UART multi-function pins (GPA10, GPA11 for normal UART)
    reg_write32(REG_GPAFUN, reg_read32(REG_GPAFUN) | 0x00F0_0000);

    // The board gates TX (transistor before the 74HC86) with GPA7.
    // Drive the level first, then switch the pin to GPIO output.
    let dout = reg_read32(REG_GPIOA_DOUT);
    reg_write32(
        REG_GPIOA_DOUT,
        if TX_ENABLE_LEVEL_HIGH { dout | TX_ENABLE_PIN } else { dout & !TX_ENABLE_PIN },
    );
    reg_write32(REG_GPAFUN, reg_read32(REG_GPAFUN) & !0x0000_C000); // GPA7 = GPIO
    reg_write32(REG_GPIOA_OMD, reg_read32(REG_GPIOA_OMD) | TX_ENABLE_PIN);

    // Reset TX/RX FIFOs
    reg_write32(REG_UART_FCR + UART_PORT, 0x07);

    // Setup baud rate (mode 3)
    let ref_clk = EXTERNAL_CRYSTAL_CLOCK;
    let baud_val = ref_clk / 115_200 - 2;
    reg_write32(REG_UART_BAUD + UART_PORT, (0x30 << 24) | baud_val);

    // Setup parity, data bits, stop bits
    reg_write32(
        REG_UART_LCR + UART_PORT,
        WB_PARITY_NONE | WB_DATA_BITS_8 | WB_STOP_BITS_1,
    );

    // Timeout register
    reg_write32(REG_UART_TOR + UART_PORT, 0x80 + 0x20);

    // Setup FIFO trigger level and enable FIFO
    reg_write32(REG_UART_FCR + UART_PORT, LEVEL_1_BYTE | 0x01);

    UART_INITIALIZED = true;
}

/// Send one byte exactly as given (no newline translation).
pub unsafe fn put_raw(c: u8) {
    // Wait until TX buffer is empty
    while reg_read32(REG_UART_FSR + UART_PORT) & TX_EMPTY == 0 {}
    reg_write8(REG_UART_THR + UART_PORT, c);
}

/// Wait until the last byte has left the shift register (FSR TE_FLAG).
/// Call before handing the UART to other code, which resets the FIFOs
/// and would cut a byte in half.
pub unsafe fn flush() {
    while reg_read32(REG_UART_FSR + UART_PORT) & TE_FLAG == 0 {}
}

/// Output a single character via UART (polling mode)
pub unsafe fn putchar(c: u8) {
    put_raw(c);
    if c == b'\n' {
        put_raw(b'\r');
    }
}

/// Return a received byte if one is waiting.
pub unsafe fn try_getc() -> Option<u8> {
    let fsr = reg_read32(REG_UART_FSR + UART_PORT);
    if fsr & RX_OVER_IF != 0 {
        reg_write32(REG_UART_FSR + UART_PORT, RX_OVER_IF);
    }
    if fsr & RX_EMPTY != 0 {
        None
    } else {
        Some(reg_read8(REG_UART_RBR + UART_PORT))
    }
}

/// Wait up to `timeout_ms` for a byte.
pub unsafe fn getc_timeout(timeout_ms: u32) -> Option<u8> {
    let start = timer::millis();
    loop {
        if let Some(c) = try_getc() {
            return Some(c);
        }
        if timer::millis().wrapping_sub(start) >= timeout_ms {
            return None;
        }
    }
}

/// Discard input until the line has been quiet for `quiet_ms`.
pub unsafe fn drain_rx(quiet_ms: u32) {
    while getc_timeout(quiet_ms).is_some() {}
}

/// Print a null-terminated string via UART
pub unsafe fn uart_puts(s: &str) {
    for byte in s.bytes() {
        putchar(byte);
    }
}

// ============================================================================
// Simple printf implementation (subset of C printf)
// ============================================================================

/// Write an unsigned 32-bit value as hex
unsafe fn print_hex(mut value: u32) {
    let mut buf: [u8; 8] = [0; 8];
    let mut idx = 0usize;

    if value == 0 {
        putchar(b'0');
        return;
    }

    while value != 0 {
        let digit = (value & 0xF) as u8;
        buf[idx] = if digit < 10 { b'0' + digit } else { b'A' + digit - 10 };
        idx += 1;
        value >>= 4;
    }

    // Print in reverse
    while idx > 0 {
        idx -= 1;
        putchar(buf[idx]);
    }
}

/// Write an unsigned 32-bit value as decimal
unsafe fn print_dec(mut value: u32) {
    let mut buf: [u8; 12] = [0; 12];
    let mut idx = 0usize;

    if value == 0 {
        putchar(b'0');
        return;
    }

    while value != 0 {
        buf[idx] = b'0' + (value % 10) as u8;
        idx += 1;
        value /= 10;
    }

    while idx > 0 {
        idx -= 1;
        putchar(buf[idx]);
    }
}

/// Simple formatted print
/// Supports: %s, %d, %x, %c
pub unsafe fn uart_printf(fmt: &str, args: &[UartArg]) {
    let bytes = fmt.as_bytes();
    let mut arg_idx = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'%' && i + 1 < bytes.len() {
            i += 1;
            if arg_idx >= args.len() {
                break;
            }
            match bytes[i] {
                b's' => {
                    if let UartArg::Str(s) = &args[arg_idx] {
                        uart_puts(s);
                    }
                    arg_idx += 1;
                }
                b'd' => {
                    if let UartArg::U32(val) = args[arg_idx] {
                        print_dec(val);
                    }
                    arg_idx += 1;
                }
                b'x' | b'X' => {
                    if let UartArg::U32(val) = args[arg_idx] {
                        print_hex(val);
                    }
                    arg_idx += 1;
                }
                b'c' => {
                    if let UartArg::U32(val) = args[arg_idx] {
                        putchar(val as u8);
                    }
                    arg_idx += 1;
                }
                _ => {
                    putchar(b'%');
                    putchar(bytes[i]);
                }
            }
        } else {
            putchar(bytes[i]);
        }
        i += 1;
    }
}

/// Argument type foruart_printf
#[derive(Clone, Copy)]
pub enum UartArg<'a> {
    U32(u32),
    Str(&'a str),
}

// ============================================================================
// Convenience macros (manually expanded - Rust doesn't support varargs)
// ============================================================================

/// Convenience wrapper for printing a format string with u32 args
#[macro_export]
macro_rules! sysprintf {
    ($fmt:literal $(, $arg:expr)*) => {{
        unsafe {
            $crate::uart::uart_printf($fmt, &[$($crate::uart::UartArg::U32($arg as u32)),*]);
        }
    }};
}

/// Convenience wrapper for printing a format string with a mix of u32 and &str
/// Usage: sysprintf!("hello %s %d %x\n", UartArg::Str("world"), UartArg::U32(42), UartArg::U32(0xABCD));
#[macro_export]
macro_rules! sysprintf_mixed {
    ($fmt:literal $(, $arg:expr)*) => {{
        unsafe {
            $crate::uart::uart_printf($fmt, &[$($arg),*]);
        }
    }};
}
