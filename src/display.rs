//! Update progress on the board's 800x480 LCD.
//!
//! The LCD stays off during a normal boot.  `writeapp` / `writeldr` turn it
//! on and show two bars: the UART download and the NAND programming.  The
//! panel setup is the HAL's (`n32903_rs::lcd`, Nextion 800x480 timing, the
//! same as ferrite-ui's bsp/n32903); the pixel clock is 16 MHz like there.
//!
//! The frame buffer sits just below the loader, so it takes that much off
//! the largest application image (see `update::max_app_size`).  No MMU, so
//! the D-cache is off and every write goes straight to SDRAM; no cache
//! maintenance is needed before the VPOST sees it.
//!
//! Turning the backlight on drives GPD0/GPD1, which are also JTAG pins: a
//! debugger attached while the loader runs from SDRAM drops off.

use n32903_rs::backlight::{self, Backlight};
use n32903_rs::lcd::{self as hal_lcd, rgb565, Lcd, PanelTiming};
use n32903_rs::registers::*;

const TIMING: PanelTiming = PanelTiming {
    pixel_clock_khz: 16_000,
    ..hal_lcd::NEXTION_800X480
};
const WIDTH: usize = TIMING.width as usize;
const FB_SIZE: u32 = Lcd::frame_buffer_size(&TIMING) as u32;

const BG: u16 = rgb565(14, 18, 28);
const TRACK: u16 = rgb565(44, 52, 68);
const TEXT: u16 = rgb565(235, 238, 245);
const DIM: u16 = rgb565(140, 150, 170);
const ACTIVE: u16 = rgb565(60, 140, 255);
const GOOD: u16 = rgb565(60, 200, 110);
const BAD: u16 = rgb565(235, 70, 60);

// Layout
const BAR_X: usize = 80;
const BAR_W: usize = WIDTH - 2 * BAR_X;
const BAR_H: usize = 36;
const TITLE_Y: usize = 50;
const STATUS_Y: usize = 410;
/// Label baseline row and bar row of each bar.
const BARS: [(usize, usize); 2] = [(140, 176), (260, 296)];
const LABELS: [&[u8]; 2] = [b"Download", b"Flash"];
/// Text scale: 5x7 font cells become 6*S x 8*S pixels.
const TITLE_SCALE: usize = 4;
const TEXT_SCALE: usize = 3;

#[derive(Clone, Copy)]
pub enum Bar {
    Download = 0,
    Flash = 1,
}

static mut LCD: Option<Lcd> = None;
static mut BACKLIGHT: Option<Backlight> = None;
/// Filled width (pixels) and shown percentage of each bar.
static mut FILLED: [usize; 2] = [0; 2];
static mut PERCENT: [u32; 2] = [0; 2];

/// Address of the frame buffer: the 4 KB aligned block right below the loader.
pub fn frame_buffer_addr() -> u32 {
    (crate::update::loader_base() - FB_SIZE) & !0xFFF
}

/// Turn the LCD on (first call only) and draw an empty progress screen.
pub unsafe fn begin(title: &str) {
    if (*core::ptr::addr_of!(LCD)).is_none() {
        // Black before the controller starts scanning
        core::ptr::write_bytes(frame_buffer_addr() as *mut u8, 0, FB_SIZE as usize);
        LCD = Some(Lcd::init(frame_buffer_addr(), &TIMING));
        let mut bl = Backlight::init(backlight::detect_variant(), backlight::DEFAULT_FREQ_HZ);
        bl.set_brightness(100);
        BACKLIGHT = Some(bl);
    }
    let Some(lcd) = lcd() else { return };

    lcd.fill(BG);
    draw_centered(lcd, TITLE_Y, title.as_bytes(), TITLE_SCALE, TEXT);
    for (i, &(label_y, bar_y)) in BARS.iter().enumerate() {
        draw_text(lcd, BAR_X, label_y, LABELS[i], TEXT_SCALE, DIM);
        lcd.fill_rect(BAR_X, bar_y, BAR_W, BAR_H, TRACK);
        FILLED[i] = 0;
        PERCENT[i] = u32::MAX;
        draw_percent(lcd, i, 0, DIM);
    }
    status("Waiting for host...");
}

/// Show `done` of `total` on `bar`.  Only draws what changed.
pub unsafe fn progress(bar: Bar, done: u32, total: u32) {
    let Some(lcd) = lcd() else { return };
    let i = bar as usize;
    let done = done.min(total) as u64;
    let total = total.max(1) as u64;

    let filled = (BAR_W as u64 * done / total) as usize;
    if filled > FILLED[i] {
        lcd.fill_rect(BAR_X + FILLED[i], BARS[i].1, filled - FILLED[i], BAR_H, ACTIVE);
    } else if filled < FILLED[i] {
        // Went back (a block failed and is being redone elsewhere)
        lcd.fill_rect(BAR_X + filled, BARS[i].1, FILLED[i] - filled, BAR_H, TRACK);
    }
    FILLED[i] = filled;
    draw_percent(lcd, i, (100 * done / total) as u32, TEXT);
}

/// Replace the status line under the bars.
pub unsafe fn status(text: &str) {
    status_bytes(text.as_bytes(), DIM);
}

/// Status line "<prefix><done> / <total> KB".
pub unsafe fn status_kb(prefix: &str, done: u32, total: u32) {
    let mut line = Line::new();
    line.push(prefix.as_bytes());
    line.push_u32((done + 1023) / 1024);
    line.push(b" / ");
    line.push_u32((total + 1023) / 1024);
    line.push(b" KB");
    status_bytes(line.as_bytes(), DIM);
}

/// Final state: bars that ran to the end turn green (or red), with a
/// closing status line.
pub unsafe fn finish(ok: bool) {
    let Some(lcd) = lcd() else { return };
    let color = if ok { GOOD } else { BAD };
    for (i, &(_, bar_y)) in BARS.iter().enumerate() {
        if FILLED[i] > 0 {
            lcd.fill_rect(BAR_X, bar_y, FILLED[i], BAR_H, color);
        }
    }
    let text: &[u8] = if ok { b"Update complete" } else { b"Update FAILED" };
    status_bytes(text, color);
}

/// Backlight off and the controller stopped, before handing the board to
/// the application (which reuses this memory and sets the LCD up itself).
pub unsafe fn shutdown() {
    if let Some(bl) = (*core::ptr::addr_of_mut!(BACKLIGHT)).as_mut() {
        bl.set_brightness(0);
    }
    if (*core::ptr::addr_of!(LCD)).is_some() {
        reg_write32(REG_LCM_LCDCCtl, reg_read32(REG_LCM_LCDCCtl) & !LCDCCtl_LCDRUN);
    }
    BACKLIGHT = None;
    LCD = None;
}

unsafe fn lcd() -> Option<&'static Lcd> {
    (*core::ptr::addr_of!(LCD)).as_ref()
}

unsafe fn status_bytes(text: &[u8], color: u16) {
    let Some(lcd) = lcd() else { return };
    lcd.fill_rect(0, STATUS_Y, WIDTH, 8 * TEXT_SCALE, BG);
    draw_centered(lcd, STATUS_Y, text, TEXT_SCALE, color);
}

/// "NN%" right-aligned above bar `i`, redrawn only when the value changes.
unsafe fn draw_percent(lcd: &Lcd, i: usize, percent: u32, color: u16) {
    if PERCENT[i] == percent {
        return;
    }
    PERCENT[i] = percent;
    let mut line = Line::new();
    line.push_u32(percent);
    line.push(b"%");
    let box_w = text_width(4, TEXT_SCALE);
    let y = BARS[i].0;
    lcd.fill_rect(BAR_X + BAR_W - box_w, y, box_w, 8 * TEXT_SCALE, BG);
    let w = text_width(line.len, TEXT_SCALE);
    draw_text(lcd, BAR_X + BAR_W - w, y, line.as_bytes(), TEXT_SCALE, color);
}

// ============================================================================
// Text
// ============================================================================

fn text_width(chars: usize, scale: usize) -> usize {
    if chars == 0 { 0 } else { (chars * 6 - 1) * scale }
}

fn draw_centered(lcd: &Lcd, y: usize, text: &[u8], scale: usize, color: u16) {
    let x = WIDTH.saturating_sub(text_width(text.len(), scale)) / 2;
    draw_text(lcd, x, y, text, scale, color);
}

/// Draw the set pixels of `text` (printable ASCII; anything else shows as
/// '?').  The background is left alone.
fn draw_text(lcd: &Lcd, x: usize, y: usize, text: &[u8], scale: usize, color: u16) {
    for (n, &c) in text.iter().enumerate() {
        let c = if (0x20..0x7F).contains(&c) { c } else { b'?' };
        let glyph = &FONT[(c - 0x20) as usize];
        let cx = x + n * 6 * scale;
        for (col, &bits) in glyph.iter().enumerate() {
            for row in 0..7 {
                if bits & (1 << row) != 0 {
                    lcd.fill_rect(cx + col * scale, y + row * scale, scale, scale, color);
                }
            }
        }
    }
}

/// Small fixed buffer for building a line of text with numbers.
struct Line {
    buf: [u8; 48],
    len: usize,
}

impl Line {
    fn new() -> Self {
        Line { buf: [0; 48], len: 0 }
    }

    fn push(&mut self, s: &[u8]) {
        for &c in s {
            if self.len < self.buf.len() {
                self.buf[self.len] = c;
                self.len += 1;
            }
        }
    }

    fn push_u32(&mut self, mut v: u32) {
        let mut digits = [0u8; 10];
        let mut n = 0;
        loop {
            digits[n] = b'0' + (v % 10) as u8;
            n += 1;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        while n > 0 {
            n -= 1;
            self.push(&digits[n..n + 1]);
        }
    }

    fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

/// 5x7 font, ASCII 0x20-0x7E.  Five columns per glyph, bit 0 = top row.
static FONT: [[u8; 5]; 95] = [
    [0x00, 0x00, 0x00, 0x00, 0x00], // ' '
    [0x00, 0x00, 0x5F, 0x00, 0x00], // !
    [0x00, 0x07, 0x00, 0x07, 0x00], // "
    [0x14, 0x7F, 0x14, 0x7F, 0x14], // #
    [0x24, 0x2A, 0x7F, 0x2A, 0x12], // $
    [0x23, 0x13, 0x08, 0x64, 0x62], // %
    [0x36, 0x49, 0x55, 0x22, 0x50], // &
    [0x00, 0x05, 0x03, 0x00, 0x00], // '
    [0x00, 0x1C, 0x22, 0x41, 0x00], // (
    [0x00, 0x41, 0x22, 0x1C, 0x00], // )
    [0x14, 0x08, 0x3E, 0x08, 0x14], // *
    [0x08, 0x08, 0x3E, 0x08, 0x08], // +
    [0x00, 0x50, 0x30, 0x00, 0x00], // ,
    [0x08, 0x08, 0x08, 0x08, 0x08], // -
    [0x00, 0x60, 0x60, 0x00, 0x00], // .
    [0x20, 0x10, 0x08, 0x04, 0x02], // /
    [0x3E, 0x51, 0x49, 0x45, 0x3E], // 0
    [0x00, 0x42, 0x7F, 0x40, 0x00], // 1
    [0x42, 0x61, 0x51, 0x49, 0x46], // 2
    [0x21, 0x41, 0x45, 0x4B, 0x31], // 3
    [0x18, 0x14, 0x12, 0x7F, 0x10], // 4
    [0x27, 0x45, 0x45, 0x45, 0x39], // 5
    [0x3C, 0x4A, 0x49, 0x49, 0x30], // 6
    [0x01, 0x71, 0x09, 0x05, 0x03], // 7
    [0x36, 0x49, 0x49, 0x49, 0x36], // 8
    [0x06, 0x49, 0x49, 0x29, 0x1E], // 9
    [0x00, 0x36, 0x36, 0x00, 0x00], // :
    [0x00, 0x56, 0x36, 0x00, 0x00], // ;
    [0x08, 0x14, 0x22, 0x41, 0x00], // <
    [0x14, 0x14, 0x14, 0x14, 0x14], // =
    [0x00, 0x41, 0x22, 0x14, 0x08], // >
    [0x02, 0x01, 0x51, 0x09, 0x06], // ?
    [0x32, 0x49, 0x79, 0x41, 0x3E], // @
    [0x7E, 0x11, 0x11, 0x11, 0x7E], // A
    [0x7F, 0x49, 0x49, 0x49, 0x36], // B
    [0x3E, 0x41, 0x41, 0x41, 0x22], // C
    [0x7F, 0x41, 0x41, 0x22, 0x1C], // D
    [0x7F, 0x49, 0x49, 0x49, 0x41], // E
    [0x7F, 0x09, 0x09, 0x09, 0x01], // F
    [0x3E, 0x41, 0x49, 0x49, 0x7A], // G
    [0x7F, 0x08, 0x08, 0x08, 0x7F], // H
    [0x00, 0x41, 0x7F, 0x41, 0x00], // I
    [0x20, 0x40, 0x41, 0x3F, 0x01], // J
    [0x7F, 0x08, 0x14, 0x22, 0x41], // K
    [0x7F, 0x40, 0x40, 0x40, 0x40], // L
    [0x7F, 0x02, 0x0C, 0x02, 0x7F], // M
    [0x7F, 0x04, 0x08, 0x10, 0x7F], // N
    [0x3E, 0x41, 0x41, 0x41, 0x3E], // O
    [0x7F, 0x09, 0x09, 0x09, 0x06], // P
    [0x3E, 0x41, 0x51, 0x21, 0x5E], // Q
    [0x7F, 0x09, 0x19, 0x29, 0x46], // R
    [0x46, 0x49, 0x49, 0x49, 0x31], // S
    [0x01, 0x01, 0x7F, 0x01, 0x01], // T
    [0x3F, 0x40, 0x40, 0x40, 0x3F], // U
    [0x1F, 0x20, 0x40, 0x20, 0x1F], // V
    [0x3F, 0x40, 0x38, 0x40, 0x3F], // W
    [0x63, 0x14, 0x08, 0x14, 0x63], // X
    [0x07, 0x08, 0x70, 0x08, 0x07], // Y
    [0x61, 0x51, 0x49, 0x45, 0x43], // Z
    [0x00, 0x7F, 0x41, 0x41, 0x00], // [
    [0x02, 0x04, 0x08, 0x10, 0x20], // backslash
    [0x00, 0x41, 0x41, 0x7F, 0x00], // ]
    [0x04, 0x02, 0x01, 0x02, 0x04], // ^
    [0x40, 0x40, 0x40, 0x40, 0x40], // _
    [0x00, 0x01, 0x02, 0x04, 0x00], // `
    [0x20, 0x54, 0x54, 0x54, 0x78], // a
    [0x7F, 0x48, 0x44, 0x44, 0x38], // b
    [0x38, 0x44, 0x44, 0x44, 0x20], // c
    [0x38, 0x44, 0x44, 0x48, 0x7F], // d
    [0x38, 0x54, 0x54, 0x54, 0x18], // e
    [0x08, 0x7E, 0x09, 0x01, 0x02], // f
    [0x0C, 0x52, 0x52, 0x52, 0x3E], // g
    [0x7F, 0x08, 0x04, 0x04, 0x78], // h
    [0x00, 0x44, 0x7D, 0x40, 0x00], // i
    [0x20, 0x40, 0x44, 0x3D, 0x00], // j
    [0x7F, 0x10, 0x28, 0x44, 0x00], // k
    [0x00, 0x41, 0x7F, 0x40, 0x00], // l
    [0x7C, 0x04, 0x18, 0x04, 0x78], // m
    [0x7C, 0x08, 0x04, 0x04, 0x78], // n
    [0x38, 0x44, 0x44, 0x44, 0x38], // o
    [0x7C, 0x14, 0x14, 0x14, 0x08], // p
    [0x08, 0x14, 0x14, 0x18, 0x7C], // q
    [0x7C, 0x08, 0x04, 0x04, 0x08], // r
    [0x48, 0x54, 0x54, 0x54, 0x20], // s
    [0x04, 0x3F, 0x44, 0x40, 0x20], // t
    [0x3C, 0x40, 0x40, 0x20, 0x7C], // u
    [0x1C, 0x20, 0x40, 0x20, 0x1C], // v
    [0x3C, 0x40, 0x30, 0x40, 0x3C], // w
    [0x44, 0x28, 0x10, 0x28, 0x44], // x
    [0x0C, 0x50, 0x50, 0x50, 0x3C], // y
    [0x44, 0x64, 0x54, 0x4C, 0x44], // z
    [0x00, 0x08, 0x36, 0x41, 0x00], // {
    [0x00, 0x00, 0x7F, 0x00, 0x00], // |
    [0x00, 0x41, 0x36, 0x08, 0x00], // }
    [0x08, 0x04, 0x08, 0x10, 0x08], // ~
];
