//! Millisecond time base for the boot countdown and UART timeouts.
//!
//! TIMER0 runs in periodic mode from the external crystal with a 1 ms
//! period.  Nothing is interrupt driven: `millis()` polls TIF0 and counts
//! elapsed periods, so it must be called at least once per millisecond to
//! stay accurate (every wait loop in this loader does that).
//!
//! If TIMER0 never fires (clock gate or source differs on this board), we
//! fall back to counting polls, which is only roughly calibrated.

use crate::clock;
use n32903_rs::registers::*;

/// Polls of `millis()` treated as one millisecond in fallback mode.
const FALLBACK_POLLS_PER_MS: u32 = 4000;

static mut MS: u32 = 0;
static mut FALLBACK: bool = false;
static mut FALLBACK_POLLS: u32 = 0;

/// Start TIMER0 with a 1 ms period.  Returns false if it is not ticking.
pub unsafe fn init() -> bool {
    MS = 0;
    FALLBACK = false;
    FALLBACK_POLLS = 0;

    reg_write32(REG_APBCLK, reg_read32(REG_APBCLK) | TMR0_CKE);
    reg_write32(REG_TCSR0, 0);
    reg_write32(REG_TISR, TIF0);

    let ticks_per_ms = clock::sys_get_external_clock(); // kHz == ticks per ms
    reg_write32(REG_TICR0, ticks_per_ms);
    reg_write32(REG_TCSR0, TCSR_CEN | TCSR_MODE_PERIODIC | TCSR_CRST);

    // One period is 1 ms; give it far longer than that before giving up.
    for _ in 0..2_000_000u32 {
        if reg_read32(REG_TISR) & TIF0 != 0 {
            reg_write32(REG_TISR, TIF0);
            return true;
        }
    }
    FALLBACK = true;
    false
}

/// Stop TIMER0 so the application starts with it idle.
pub unsafe fn deinit() {
    reg_write32(REG_TCSR0, 0);
    reg_write32(REG_TISR, TIF0);
}

/// Milliseconds since `init()`.
pub unsafe fn millis() -> u32 {
    if FALLBACK {
        FALLBACK_POLLS += 1;
        if FALLBACK_POLLS >= FALLBACK_POLLS_PER_MS {
            FALLBACK_POLLS = 0;
            MS = MS.wrapping_add(1);
        }
    } else if reg_read32(REG_TISR) & TIF0 != 0 {
        reg_write32(REG_TISR, TIF0);
        MS = MS.wrapping_add(1);
    }
    MS
}
