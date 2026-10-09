//! SPU/DAC depop initialization
//! Ported from Depop.c

use crate::registers::*;

/// Simple delay (~10us per iteration at 96MHz)
unsafe fn sys_delay(count: u32) {
    spin!(count * 10000);
}

/// Initialize SPU DAC for pop suppression
pub unsafe fn spu_dac_on(level: u8) {
    // Disable then enable SPU engine clock
    reg_write32(
        REG_AHBCLK,
        reg_read32(REG_AHBCLK) & !(ADO_CKE | SPU_CKE | HCLK4_CKE),
    );
    reg_write32(
        REG_AHBCLK,
        reg_read32(REG_AHBCLK) | ADO_CKE | SPU_CKE | HCLK4_CKE,
    );

    // Reset SPU
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPURST);
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) | SPURST);
    reg_write32(REG_SPU_CTRL, reg_read32(REG_SPU_CTRL) & !SPURST);

    // Configure DAC parameters
    reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) | 0x30); // disable

    match level {
        3 => reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) & !0x30), // delay 3s
        1 => reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) & !0x20), // delay 0.5-1s
        2 => reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) & !0x10), // delay 2s
        _ => {
            reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x00FF_0000);
            reg_write32(REG_SPU_DAC_PAR, reg_read32(REG_SPU_DAC_PAR) | 0x30);
            return;
        }
    }

    // Step-by-step DAC power-on sequence
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x0800_0000); // P7
    sys_delay(1);
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x0400_0000); // P6
    sys_delay(1);
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x01E0_0000); // P1-4
    sys_delay(1);
    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x0200_0000); // P5
    sys_delay(1);

    reg_write32(REG_SPU_DAC_VOL, reg_read32(REG_SPU_DAC_VOL) & !0x0001_0000); // P0
}
