//! Boot countdown and the interactive command line.

use crate::timer;
use crate::uart::{self, UartArg};
use crate::update;

const PROMPT: &str = "n32903> ";
const AUTOBOOT_SECONDS: u32 = 3;
const LINE_MAX: usize = 32;

/// Show the countdown.  True if a key was pressed (the key is discarded).
pub unsafe fn autoboot_interrupted() -> bool {
    uart::uart_printf(
        "Press any key to enter menu : %d",
        &[UartArg::U32(AUTOBOOT_SECONDS)],
    );
    for remaining in (0..AUTOBOOT_SECONDS).rev() {
        let start = timer::millis();
        while timer::millis().wrapping_sub(start) < 1000 {
            if uart::try_getc().is_some() {
                // Swallow the rest of what was typed (e.g. "\r\n").
                uart::drain_rx(100);
                uart::uart_puts("\n");
                return true;
            }
        }
        uart::uart_printf("\x08%d", &[UartArg::U32(remaining)]);
    }
    uart::uart_puts("\n");
    false
}

/// Run the command line until the user types `boot`.
pub unsafe fn run() {
    uart::uart_puts("Type 'help' for commands.\n");
    let mut line = [0u8; LINE_MAX];
    let mut last_was_cr = false;
    loop {
        uart::uart_puts(PROMPT);
        let len = read_line(&mut line, &mut last_was_cr);
        let cmd = core::str::from_utf8(&line[..len]).unwrap_or("").trim();
        match cmd {
            "" => {}
            "boot" => return,
            "writeapp" => update::cmd_writeapp(),
            "writeldr" => update::cmd_writeldr(),
            "help" | "?" => uart::uart_puts(
                "  writeapp  - receive application image over UART and write it to NAND\n\
                 \x20 writeldr  - receive loader image over UART and write it to blocks 0-3\n\
                 \x20 boot      - load and run the application\n\
                 \x20 help      - show this list (also '?')\n",
            ),
            _ => uart::uart_printf(
                "Unknown command '%s'. Type 'help'.\n",
                &[UartArg::Str(cmd)],
            ),
        }
    }
}

/// Read one line with echo and backspace.  Accepts CR, LF or CRLF.
unsafe fn read_line(buf: &mut [u8; LINE_MAX], last_was_cr: &mut bool) -> usize {
    let mut len = 0;
    loop {
        let Some(c) = uart::try_getc() else { continue };
        match c {
            b'\n' if *last_was_cr => {
                // LF of a CRLF pair; the line already ended on CR.
                *last_was_cr = false;
            }
            b'\r' | b'\n' => {
                *last_was_cr = c == b'\r';
                uart::uart_puts("\n");
                return len;
            }
            0x08 | 0x7F => {
                *last_was_cr = false;
                if len > 0 {
                    len -= 1;
                    uart::uart_puts("\x08 \x08");
                }
            }
            0x03 => {
                *last_was_cr = false;
                uart::uart_puts("^C\n");
                return 0;
            }
            0x20..=0x7E => {
                *last_was_cr = false;
                if len < LINE_MAX {
                    buf[len] = c;
                    len += 1;
                    uart::putchar(c);
                }
            }
            _ => *last_was_cr = false,
        }
    }
}
