//! NAND Flash driver for N32903 (FMI/SM interface)
//! Ported from NandDrv.c + turbowriter.h

use n32903_rs::registers::*;
use crate::sysprintf;

// ============================================================================
// Constants
// ============================================================================

pub const NAND_PAGE_512B: u32 = 512;
pub const NAND_PAGE_2KB: u32 = 2048;
pub const NAND_PAGE_4KB: u32 = 4096;

pub const SUCCESSFUL: i32 = 0;

// ============================================================================
// NAND Flash Information Structure (from turbowriter.h: FMI_SM_INFO_T)
// ============================================================================

#[derive(Clone)]
pub struct FmiSmInfo {
    pub sector_per_flash: u32,
    pub block_per_flash: u32,
    pub page_per_block: u32,
    pub sector_per_block: u32,
    pub lib_start_block: u32,
    pub page_size: u32,
    pub bad_block_count: u32,
    pub is_multicycle: bool,
    pub is_mlc_nand: bool,
    pub is_nand_ecc4: bool,
    pub is_nand_ecc8: bool,
    pub is_nand_ecc12: bool,
    pub is_nand_ecc15: bool,
    pub is_check_ecc: bool,
}

impl Default for FmiSmInfo {
    fn default() -> Self {
        FmiSmInfo {
            sector_per_flash: 0,
            block_per_flash: 0,
            page_per_block: 0,
            sector_per_block: 0,
            lib_start_block: 0,
            page_size: 0,
            bad_block_count: 0,
            is_multicycle: false,
            is_mlc_nand: false,
            is_nand_ecc4: false,
            is_nand_ecc8: false,
            is_nand_ecc12: false,
            is_nand_ecc15: false,
            is_check_ecc: true,
        }
    }
}

// ============================================================================
// Global NAND state
// ============================================================================

static mut NAND_INITIALIZED: bool = false;
static mut SM0: FmiSmInfo = FmiSmInfo {
    sector_per_flash: 0,
    block_per_flash: 0,
    page_per_block: 0,
    sector_per_block: 0,
    lib_start_block: 0,
    page_size: 0,
    bad_block_count: 0,
    is_multicycle: false,
    is_mlc_nand: false,
    is_nand_ecc4: false,
    is_nand_ecc8: false,
    is_nand_ecc12: false,
    is_nand_ecc15: false,
    is_check_ecc: true,
};

// ============================================================================
// SM Hardware Operations
// ============================================================================

/// Check Ready/Busy pin status
unsafe fn fmi_sm_check_rb() -> i32 {
    let mut timeout: u32 = 0;
    loop {
        if reg_read32(REG_SMISR) & SMISR_RB0_IF != 0 {
            reg_write32(REG_SMISR, SMISR_RB0_IF);
            return 1;
        }
        timeout += 1;
        if timeout > 0x80000 {
            return 0;
        }
    }
}

/// Reset SM controller
unsafe fn fmi_sm_reset() -> i32 {
    reg_write32(REG_SMCMD, 0xFF);
    spin!(100);
    if fmi_sm_check_rb() == 0 {
        return -1;
    }
    0
}

// ============================================================================
// SM Initialization (page size, ECC)
// ============================================================================

unsafe fn fmi_sm_initial(sm: &mut FmiSmInfo) {
    if sm.page_size == NAND_PAGE_2KB {
        // 2K page
        reg_write32(REG_SMCSR, (reg_read32(REG_SMCSR) & !SMCR_PSIZE) | PSIZE_2K);
        reg_write32(REG_SMCSR, (reg_read32(REG_SMCSR) & !SMCR_BCH_TSEL) | BCH_T4);
        reg_write32(REG_SMREAREA_CTL, 64); // redundant area: 64 bytes
    } else if sm.page_size == NAND_PAGE_512B {
        // 512B page
        reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) & !SMCR_PSIZE);
        reg_write32(REG_SMCSR, (reg_read32(REG_SMCSR) & !SMCR_BCH_TSEL) | BCH_T4);
        reg_write32(REG_SMREAREA_CTL, 16); // redundant area: 16 bytes
    } else {
        // 4K page
        reg_write32(REG_SMCSR, (reg_read32(REG_SMCSR) & !SMCR_PSIZE) | PSIZE_4K);
        reg_write32(REG_SMCSR, (reg_read32(REG_SMCSR) & !SMCR_BCH_TSEL) | BCH_T8);
        reg_write32(REG_SMREAREA_CTL, 128); // redundant area: 128 bytes
    }
}

// ============================================================================
// SM Read ID - Identifies NAND chip and configures parameters
// ============================================================================

unsafe fn fmi_sm_read_id(sm: &mut FmiSmInfo) -> i32 {
    let mut temp_id: [u32; 5] = [0; 5];

    fmi_sm_reset();
    reg_write32(REG_SMCMD, 0x90); // Read ID command
    reg_write32(REG_SMADDR, EOA_SM); // address 0x00

    temp_id[0] = reg_read32(REG_SMDATA);
    temp_id[1] = reg_read32(REG_SMDATA);
    temp_id[2] = reg_read32(REG_SMDATA);
    temp_id[3] = reg_read32(REG_SMDATA);
    temp_id[4] = reg_read32(REG_SMDATA);

    // Check if NAND ROM (no ECC support)
    if (temp_id[0] == 0xC2 && temp_id[1] == 0x79)
        || (temp_id[0] == 0xC2 && temp_id[1] == 0x76)
    {
        sm.is_check_ecc = false;
    } else {
        sm.is_check_ecc = true;
    }

    sm.is_nand_ecc4 = false;
    sm.is_nand_ecc8 = false;
    sm.is_nand_ecc12 = false;
    sm.is_nand_ecc15 = false;

    match temp_id[1] {
        // --- 512B page size ---
        0x79 => {
            // 128M
            sm.sector_per_flash = 255744;
            sm.block_per_flash = 8191;
            sm.page_per_block = 32;
            sm.sector_per_block = 32;
            sm.is_multicycle = true;
            sm.page_size = NAND_PAGE_512B;
            sm.is_nand_ecc4 = true;
        }
        0x76 => {
            // 64M
            sm.sector_per_flash = 127872;
            sm.block_per_flash = 4095;
            sm.page_per_block = 32;
            sm.sector_per_block = 32;
            sm.is_multicycle = true;
            sm.page_size = NAND_PAGE_512B;
            sm.is_nand_ecc4 = true;
        }
        0x75 => {
            // 32M
            sm.sector_per_flash = 63936;
            sm.block_per_flash = 2047;
            sm.page_per_block = 32;
            sm.sector_per_block = 32;
            sm.is_multicycle = false;
            sm.page_size = NAND_PAGE_512B;
            sm.is_nand_ecc4 = true;
        }
        0x73 => {
            // 16M
            sm.sector_per_flash = 31968;
            sm.block_per_flash = 1023;
            sm.page_per_block = 32;
            sm.sector_per_block = 32;
            sm.is_multicycle = false;
            sm.page_size = NAND_PAGE_512B;
            sm.is_nand_ecc4 = true;
        }

        // --- 2KB page size ---
        0xF1 | 0xD1 => {
            // 128M
            sm.block_per_flash = 1023;
            sm.page_per_block = 64;
            sm.sector_per_block = 256;
            sm.sector_per_flash = 255744;
            sm.is_multicycle = false;
            sm.page_size = NAND_PAGE_2KB;
            sm.is_nand_ecc8 = true;
            // Check for MXIC flashes that support ECC
            if (temp_id[0] == 0xC2
                && temp_id[1] == 0xF1
                && temp_id[2] == 0x80
                && temp_id[3] == 0x1D)
                || (temp_id[0] == 0xC2
                    && temp_id[1] == 0xF1
                    && temp_id[2] == 0x80
                    && temp_id[3] == 0x95
                    && temp_id[4] == 0x02)
            {
                sm.is_check_ecc = true;
            }
        }
        0xDA => {
            // 256M
            if temp_id[3] & 0x33 == 0x11 {
                sm.block_per_flash = 2047;
                sm.page_per_block = 64;
                sm.sector_per_block = 256;
                sm.is_mlc_nand = false;
            } else if temp_id[3] & 0x33 == 0x21 {
                sm.block_per_flash = 1023;
                sm.page_per_block = 128;
                sm.sector_per_block = 512;
                sm.is_mlc_nand = true;
            }
            sm.sector_per_flash = 511488;
            sm.is_multicycle = true;
            sm.page_size = NAND_PAGE_2KB;
            sm.is_nand_ecc8 = true;
            if temp_id[0] == 0xC2
                && temp_id[1] == 0xDA
                && temp_id[2] == 0x90
                && temp_id[3] == 0x95
                && temp_id[4] == 0x06
            {
                sm.is_check_ecc = true;
            }
        }
        0xDC => {
            // 512M
            // Micron MT29F4G08ABAEA (4KB page)
            if temp_id[0] == 0x2C
                && temp_id[2] == 0x90
                && temp_id[3] == 0xA6
                && temp_id[4] == 0x54
            {
                sm.block_per_flash = 2047;
                sm.page_per_block = 64;
                sm.page_size = NAND_PAGE_4KB;
                sm.sector_per_block = sm.page_size / 512 * sm.page_per_block;
                sm.is_mlc_nand = false;
                sm.is_multicycle = true;
                sm.is_nand_ecc12 = true;
                sm.sector_per_flash = sm.sector_per_block * 2000 / 1000 * 999;
                return 0;
            }
            // Toshiba/KFM 512MB (4KB page, ECC8)
            if temp_id[0] == 0x98
                && temp_id[2] == 0x90
                && temp_id[3] == 0x26
                && temp_id[4] == 0x76
            {
                sm.block_per_flash = 2047;
                sm.page_per_block = 64;
                sm.page_size = NAND_PAGE_4KB;
                sm.sector_per_block = sm.page_size / 512 * sm.page_per_block;
                sm.is_mlc_nand = false;
                sm.is_multicycle = true;
                sm.is_nand_ecc8 = true;
                sm.sector_per_flash = 1022976;
                return 0;
            }
            // Default 512M (2KB page)
            if temp_id[3] & 0x33 == 0x11 {
                sm.block_per_flash = 4095;
                sm.page_per_block = 64;
                sm.sector_per_block = 256;
                sm.is_mlc_nand = false;
            } else if temp_id[3] & 0x33 == 0x21 {
                sm.block_per_flash = 2047;
                sm.page_per_block = 128;
                sm.sector_per_block = 512;
                sm.is_mlc_nand = true;
            }
            sm.sector_per_flash = 1022976;
            sm.is_multicycle = true;
            sm.page_size = NAND_PAGE_2KB;
            sm.is_nand_ecc8 = true;
        }
        0xD3 => {
            // 1024M
            sm.is_mlc_nand = false;
            sm.is_multicycle = true;
            sm.is_nand_ecc8 = true;
            sm.sector_per_flash = 2045952;
            // Samsung K9WAG08U1D
            if temp_id[0] == 0xEC
                && temp_id[2] == 0x51
                && temp_id[3] == 0x95
                && temp_id[4] == 0x58
            {
                sm.block_per_flash = 4095;
                sm.page_per_block = 64;
                sm.page_size = NAND_PAGE_2KB;
                sm.sector_per_block = sm.page_size / 512 * sm.page_per_block;
                sm.sector_per_flash = 1022976;
                return 0;
            }
            // MXIC MX60LF8G18AC
            if temp_id[0] == 0xC2
                && temp_id[2] == 0xD1
                && temp_id[3] == 0x95
                && temp_id[4] == 0x5A
            {
                sm.is_check_ecc = true;
            }
            if temp_id[3] & 0x33 == 0x32 {
                sm.block_per_flash = 2047;
                sm.page_per_block = 128;
                sm.sector_per_block = 1024;
                sm.page_size = NAND_PAGE_4KB;
                sm.is_mlc_nand = true;
            } else if temp_id[3] & 0x33 == 0x11 {
                sm.block_per_flash = 8191;
                sm.page_per_block = 64;
                sm.sector_per_block = 256;
                sm.page_size = NAND_PAGE_2KB;
            } else if temp_id[3] & 0x33 == 0x21 {
                sm.block_per_flash = 4095;
                sm.page_per_block = 128;
                sm.sector_per_block = 512;
                sm.page_size = NAND_PAGE_2KB;
                sm.is_mlc_nand = true;
            } else if temp_id[3] & 0x33 == 0x22 {
                sm.block_per_flash = 4095;
                sm.page_per_block = 64;
                sm.sector_per_block = 512;
                sm.page_size = NAND_PAGE_4KB;
            }
        }
        0xD5 => {
            // 2048M
            if temp_id[3] & 0x33 == 0x32 {
                sm.block_per_flash = 4095;
                sm.page_per_block = 128;
                sm.sector_per_block = 1024;
                sm.page_size = NAND_PAGE_4KB;
                sm.is_mlc_nand = true;
            } else if temp_id[3] & 0x33 == 0x11 {
                sm.block_per_flash = 16383;
                sm.page_per_block = 64;
                sm.sector_per_block = 256;
                sm.page_size = NAND_PAGE_2KB;
                sm.is_mlc_nand = false;
            } else if temp_id[3] & 0x33 == 0x21 {
                sm.block_per_flash = 8191;
                sm.page_per_block = 128;
                sm.sector_per_block = 512;
                sm.page_size = NAND_PAGE_2KB;
                sm.is_mlc_nand = true;
            }
            sm.sector_per_flash = 4091904;
            sm.is_multicycle = true;
            sm.is_nand_ecc8 = true;
        }
        _ => {
            // MXIC MX30LF1208AA
            if temp_id[0] == 0xC2
                && temp_id[1] == 0xF0
                && temp_id[2] == 0x80
                && temp_id[3] == 0x1D
            {
                sm.block_per_flash = 511;
                sm.page_per_block = 64;
                sm.page_size = NAND_PAGE_2KB;
                sm.sector_per_block = sm.page_size / 512 * sm.page_per_block;
                sm.is_mlc_nand = false;
                sm.is_multicycle = false;
                sm.is_nand_ecc8 = true;
                sm.sector_per_flash = sm.sector_per_block * 500 / 1000 * 999;
                sm.is_check_ecc = true;
                return 0;
            }
            // Unknown NAND
            sysprintf!("ERROR: SM ID not support!! [%x][%x][%x][%x][%x]\n",
                temp_id[0], temp_id[1], temp_id[2], temp_id[3], temp_id[4]);
            return -1;
        }
    }

    sysprintf!("SM ID [%x][%x][%x][%x][%x]\n",
        temp_id[0], temp_id[1], temp_id[2], temp_id[3], temp_id[4]);
    0
}

// ============================================================================
// SM Read Operations
// ============================================================================

/// Send read command to NAND for 2K/4K page
unsafe fn fmi_sm2_buffer_m(sm: &FmiSmInfo, sector: u32, col_addr: u8) -> i32 {
    // wait for R/B
    while reg_read32(REG_SMISR) & SMISR_RB0 == 0 {}
    reg_write32(REG_SMISR, SMISR_RB0_IF);

    reg_write32(REG_SMCMD, 0x00); // Read command
    reg_write32(REG_SMADDR, col_addr as u32); // CA0-CA7
    reg_write32(REG_SMADDR, sector & 0xFF); // PA0-PA7
    if !sm.is_multicycle {
        reg_write32(REG_SMADDR, ((sector >> 8) & 0xFF) | EOA_SM); // PA8-PA15
    } else {
        reg_write32(REG_SMADDR, (sector >> 8) & 0xFF); // PA8-PA15
        reg_write32(REG_SMADDR, ((sector >> 16) & 0xFF) | EOA_SM); // PA16-PA17
    }

    if fmi_sm_check_rb() == 0 {
        return -1;
    }
    0
}

/// Read redundant area and page data for a page
unsafe fn fmi_sm_read_ra(sm: &FmiSmInfo, page: u32, col_addr: u32) -> i32 {
    while reg_read32(REG_SMISR) & SMISR_RB0 == 0 {}
    reg_write32(REG_SMISR, SMISR_RB0_IF);

    reg_write32(REG_SMCMD, 0x00); // Read command
    reg_write32(REG_SMADDR, col_addr); // CA0-CA7
    reg_write32(REG_SMADDR, (col_addr >> 8) & 0xFF); // CA8-CA12
    reg_write32(REG_SMADDR, page & 0xFF); // PA0-PA7
    if !sm.is_multicycle {
        reg_write32(REG_SMADDR, ((page >> 8) & 0xFF) | EOA_SM);
    } else {
        reg_write32(REG_SMADDR, (page >> 8) & 0xFF);
        reg_write32(REG_SMADDR, ((page >> 16) & 0xFF) | EOA_SM);
    }
    reg_write32(REG_SMCMD, 0x30); // Read confirm command

    if fmi_sm_check_rb() == 0 {
        return -1;
    }
    0
}

/// Read redundant area for 512B page
unsafe fn fmi_sm_read_ra_512(sm: &FmiSmInfo, page: u32, column: u32) -> i32 {
    while reg_read32(REG_SMISR) & SMISR_RB0 == 0 {}
    reg_write32(REG_SMISR, SMISR_RB0_IF);

    reg_write32(REG_SMCMD, 0x50); // Read spare area command
    reg_write32(REG_SMADDR, column);
    reg_write32(REG_SMADDR, page & 0xFF);
    if !sm.is_multicycle {
        reg_write32(REG_SMADDR, ((page >> 8) & 0xFF) | EOA_SM);
    } else {
        reg_write32(REG_SMADDR, (page >> 8) & 0xFF);
        reg_write32(REG_SMADDR, ((page >> 16) & 0xFF) | EOA_SM);
    }

    if fmi_sm_check_rb() == 0 {
        return -1;
    }
    0
}

// ============================================================================
// BCH ECC Error Correction
// ============================================================================

/// Correct data using BCH ECC engine
unsafe fn fmi_sm_correct_data_bch(field_index: u8, error_cnt: u8, daddr: u32) {
    let mut ua_data: [u32; 16] = [0; 16];
    let mut ua_addr: [u32; 16] = [0; 16];
    let mut ua_error_data: [u32; 4] = [0; 4];

    let page_size = reg_read32(REG_SMCSR) & SMCR_PSIZE;

    // Get error data
    let mut jj = error_cnt / 4 + 1;
    if jj > 4 {
        jj = 4;
    }

    for ii in 0..jj as usize {
        ua_error_data[ii] = reg_read32(REG_BCH_ECC_DATA0 + (ii * 4) as u32);
    }

    for ii in 0..jj as usize {
        ua_data[ii * 4] = ua_error_data[ii] & 0xFF;
        ua_data[ii * 4 + 1] = (ua_error_data[ii] >> 8) & 0xFF;
        ua_data[ii * 4 + 2] = (ua_error_data[ii] >> 16) & 0xFF;
        ua_data[ii * 4 + 3] = (ua_error_data[ii] >> 24) & 0xFF;
    }

    // Get error addresses
    jj = error_cnt / 2 + 1;
    if jj > 8 {
        jj = 8;
    }

    for ii in 0..jj as usize {
        ua_addr[ii * 2] = reg_read32(REG_BCH_ECC_ADDR0 + (ii * 4) as u32) & 0x1FFF;
        ua_addr[ii * 2 + 1] = (reg_read32(REG_BCH_ECC_ADDR0 + (ii * 4) as u32) >> 16) & 0x1FFF;
    }

    let p_daddr = (daddr + (field_index as u32 - 1) * 0x200) as *mut u8;

    for ii in 0..error_cnt as usize {
        let bch_tsel = reg_read32(REG_SMCSR) & SMCR_BCH_TSEL;
        if page_size == PSIZE_4K {
            if bch_tsel & BCH_T12 != 0 {
                if ua_addr[ii] < 512 {
                    *p_daddr.add(ua_addr[ii] as usize) ^= ua_data[ii] as u8;
                } else if ua_addr[ii] < 515 {
                    let offset = ua_addr[ii] - 512 + 23 * (field_index as u32 - 1);
                    *((REG_SMRA_0 + offset) as *mut u8) ^= ua_data[ii] as u8;
                } else {
                    let offset = 22 - (543 - ua_addr[ii]) + 23 * (field_index as u32 - 1);
                    *((REG_SMRA_0 + 8 * 4 + offset) as *mut u8) ^= ua_data[ii] as u8;
                }
            } else if bch_tsel & BCH_T8 != 0 {
                if ua_addr[ii] < 512 {
                    *p_daddr.add(ua_addr[ii] as usize) ^= ua_data[ii] as u8;
                } else if ua_addr[ii] < 515 {
                    let offset = ua_addr[ii] - 512 + 15 * (field_index as u32 - 1);
                    *((REG_SMRA_0 + offset) as *mut u8) ^= ua_data[ii] as u8;
                } else {
                    let offset = 14 - (543 - ua_addr[ii]) + 15 * (field_index as u32 - 1);
                    *((REG_SMRA_0 + 2 * 4 + offset) as *mut u8) ^= ua_data[ii] as u8;
                }
            } else {
                // BCH_T4 (default)
                if ua_addr[ii] < 512 {
                    *p_daddr.add(ua_addr[ii] as usize) ^= ua_data[ii] as u8;
                } else if ua_addr[ii] < 515 {
                    let offset = ua_addr[ii] - 512 + 8 * (field_index as u32 - 1);
                    *((REG_SMRA_0 + offset) as *mut u8) ^= ua_data[ii] as u8;
                } else {
                    let offset = 7 - (543 - ua_addr[ii]) + 8 * (field_index as u32 - 1);
                    *((REG_SMRA_0 + 16 * 4 + offset) as *mut u8) ^= ua_data[ii] as u8;
                }
            }
        } else if page_size == PSIZE_2K {
            if bch_tsel & BCH_T8 != 0 {
                if ua_addr[ii] < 512 {
                    *p_daddr.add(ua_addr[ii] as usize) ^= ua_data[ii] as u8;
                } else if ua_addr[ii] < 515 {
                    let offset = ua_addr[ii] - 512 + 15 * (field_index as u32 - 1);
                    *((REG_SMRA_0 + offset) as *mut u8) ^= ua_data[ii] as u8;
                } else {
                    let offset = 14 - (543 - ua_addr[ii]) + 15 * (field_index as u32 - 1);
                    *((REG_SMRA_0 + 4 + offset) as *mut u8) ^= ua_data[ii] as u8;
                }
            } else {
                // BCH_T4 (default)
                if ua_addr[ii] < 512 {
                    *p_daddr.add(ua_addr[ii] as usize) ^= ua_data[ii] as u8;
                } else if ua_addr[ii] < 515 {
                    let offset = ua_addr[ii] - 512 + 8 * (field_index as u32 - 1);
                    *((REG_SMRA_0 + offset) as *mut u8) ^= ua_data[ii] as u8;
                } else {
                    let offset = 7 - (543 - ua_addr[ii]) + 8 * (field_index as u32 - 1);
                    *((REG_SMRA_0 + 8 * 4 + offset) as *mut u8) ^= ua_data[ii] as u8;
                }
            }
        } else {
            // 512B page
            if ua_addr[ii] < 512 {
                *p_daddr.add(ua_addr[ii] as usize) ^= ua_data[ii] as u8;
            } else if ua_addr[ii] < 515 {
                let offset = ua_addr[ii] - 512;
                *((REG_SMRA_0 + offset) as *mut u8) ^= ua_data[ii] as u8;
            } else {
                let offset = 7 - (543 - ua_addr[ii]);
                *((REG_SMRA_0 + 2 * 4 + offset) as *mut u8) ^= ua_data[ii] as u8;
            }
        }
    }
}

/// Read 512B page data
unsafe fn fmi_sm_read_512(sm: &FmiSmInfo, sector: u32, daddr: u32) -> i32 {
    let mut uerror: u32 = 0;

    reg_write32(REG_DMACSAR, daddr);
    if fmi_sm2_buffer_m(sm, sector, 0) < 0 {
        return -1;
    }

    reg_write32(REG_SMISR, SMISR_DMA_IF);
    reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_DRD_EN);

    while reg_read32(REG_SMCSR) & SMCR_DRD_EN != 0 {}

    if sm.is_check_ecc {
        loop {
            if reg_read32(REG_SMISR) & SMISR_ECC_FIELD_IF != 0 {
                if reg_read32(REG_SMCSR) & BCH_T4 != 0 {
                    let ustatus = reg_read32(REG_SM_ECC_ST0) & 0x3F;
                    match ustatus & 0x03 {
                        0x01 => {
                            // Correctable error
                            let error_cnt = ustatus >> 2;
                            fmi_sm_correct_data_bch(1, error_cnt as u8, daddr);
                        }
                        0x02 | 0x03 => {
                            // Uncorrectable
                            uerror = 1;
                        }
                        _ => {}
                    }
                }
                reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
            }
            if reg_read32(REG_SMISR) & SMISR_DMA_IF != 0 {
                if reg_read32(REG_SMISR) & SMISR_ECC_FIELD_IF == 0 {
                    break;
                }
            }
        }
    } else {
        reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
    }

    reg_write32(REG_SMISR, SMISR_DMA_IF);
    if uerror != 0 {
        -1
    } else {
        0
    }
}

/// Read 2K page data
unsafe fn fmi_sm_read_2k(sm: &FmiSmInfo, page: u32, daddr: u32) -> i32 {
    let mut uerror: u32 = 0;

    while reg_read32(REG_DMACCSR) & FMI_BUSY != 0 {}
    reg_write32(REG_DMACCSR, reg_read32(REG_DMACCSR) | DMAC_EN);
    reg_write32(REG_DMACSAR, daddr);

    // Wait for R/B
    while reg_read32(REG_SMISR) & SMISR_RB0 == 0 {}
    reg_write32(REG_SMISR, SMISR_RB0_IF);

    if reg_read32(REG_SMISR) & SMISR_ECC_FIELD_IF != 0 {
        reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
    }

    reg_write32(REG_SMCMD, 0x00); // Read command
    reg_write32(REG_SMADDR, 0); // CA0-CA7
    reg_write32(REG_SMADDR, 0); // CA8-CA11
    reg_write32(REG_SMADDR, page & 0xFF); // PA0-PA7
    if !sm.is_multicycle {
        reg_write32(REG_SMADDR, ((page >> 8) & 0xFF) | EOA_SM);
    } else {
        reg_write32(REG_SMADDR, (page >> 8) & 0xFF);
        reg_write32(REG_SMADDR, ((page >> 16) & 0xFF) | EOA_SM);
    }
    reg_write32(REG_SMCMD, 0x30);

    if fmi_sm_check_rb() == 0 {
        return -1;
    }

    reg_write32(REG_SMISR, SMISR_DMA_IF);
    reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_DRD_EN);

    if sm.is_check_ecc {
        loop {
            if reg_read32(REG_SMISR) & SMISR_ECC_FIELD_IF != 0 {
                let mut ustatus = reg_read32(REG_SM_ECC_ST0);
                for ii in 1u8..5 {
                    match ustatus & 0x03 {
                        0x01 => {
                            let error_cnt = ustatus >> 2;
                            fmi_sm_correct_data_bch(ii, error_cnt as u8, daddr);
                            break;
                        }
                        0x02 | 0x03 => {
                            uerror = 1;
                            break;
                        }
                        _ => {}
                    }
                    ustatus >>= 8;
                }
                reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
            }
            if reg_read32(REG_SMISR) & SMISR_DMA_IF != 0 {
                if reg_read32(REG_SMISR) & SMISR_ECC_FIELD_IF == 0 {
                    break;
                }
            }
        }
    } else {
        loop {
            reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
            if reg_read32(REG_SMISR) & SMISR_DMA_IF != 0 {
                reg_write32(REG_SMISR, SMISR_DMA_IF);
                break;
            }
        }
    }

    reg_write32(REG_SMISR, SMISR_DMA_IF);
    if uerror != 0 {
        -1
    } else {
        0
    }
}

/// Read 4K page data
unsafe fn fmi_sm_read_4k(sm: &FmiSmInfo, page: u32, daddr: u32) -> i32 {
    reg_write32(REG_DMACSAR, daddr);

    // Wait for R/B
    while reg_read32(REG_SMISR) & SMISR_RB0 == 0 {}
    reg_write32(REG_SMISR, SMISR_RB0_IF);

    reg_write32(REG_SMCMD, 0x00);
    reg_write32(REG_SMADDR, 0);
    reg_write32(REG_SMADDR, 0);
    reg_write32(REG_SMADDR, page & 0xFF);
    if !sm.is_multicycle {
        reg_write32(REG_SMADDR, ((page >> 8) & 0xFF) | EOA_SM);
    } else {
        reg_write32(REG_SMADDR, (page >> 8) & 0xFF);
        reg_write32(REG_SMADDR, ((page >> 16) & 0xFF) | EOA_SM);
    }
    reg_write32(REG_SMCMD, 0x30);

    if fmi_sm_check_rb() == 0 {
        return -1;
    }

    reg_write32(REG_SMISR, SMISR_DMA_IF);
    reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_DRD_EN);

    let mut uerror: u32 = 0;

    if sm.is_check_ecc || reg_read32(REG_SMCSR) & SMCR_ECC_CHK != 0 {
        loop {
            if reg_read32(REG_SMISR) & SMISR_ECC_FIELD_IF != 0 {
                for jj in 0u32..2 {
                    let mut ustatus = reg_read32(REG_SM_ECC_ST0 + jj * 4);
                    if ustatus == 0 {
                        continue;
                    }
                    for ii in 1u8..5 {
                        if ustatus & 0x03 == 0 {
                            ustatus >>= 8;
                            continue;
                        }
                        match ustatus & 0x03 {
                            0x01 => {
                                let error_cnt = ustatus >> 2;
                                fmi_sm_correct_data_bch(
                                    (jj * 4 + ii as u32) as u8,
                                    error_cnt as u8,
                                    daddr,
                                );
                                break;
                            }
                            0x02 | 0x03 => {
                                uerror = 1;
                                break;
                            }
                            _ => {}
                        }
                        ustatus >>= 8;
                    }
                }
                reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
            }
            if reg_read32(REG_SMISR) & SMISR_DMA_IF != 0 {
                if reg_read32(REG_SMISR) & SMISR_ECC_FIELD_IF == 0 {
                    break;
                }
            }
        }
    } else {
        loop {
            reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
            if reg_read32(REG_SMISR) & SMISR_DMA_IF != 0 {
                reg_write32(REG_SMISR, SMISR_DMA_IF);
                break;
            }
        }
    }

    if uerror != 0 {
        -1
    } else {
        0
    }
}

// ============================================================================
// SM Initialization Entry Point
// ============================================================================

/// Select SM chip select
unsafe fn sic_sm_select(chip_sel: i32) {
    if chip_sel == 0 {
        // Enable CS0 pins
        reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0x0003_CC00);
        reg_write32(REG_GPEFUN, reg_read32(REG_GPEFUN) | 0x00F3_0000);
        reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) & !SMCR_CS0);
        reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_CS1);
    } else {
        // Enable CS1 pins
        reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0x0003_F000);
        reg_write32(REG_GPEFUN, reg_read32(REG_GPEFUN) | 0x00FC_0000);
        reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) & !SMCR_CS1);
        reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_CS0);
    }
}

/// ECC settings saved while a boot block is accessed with BCH T4.
struct EccSave {
    tsel: u32,
    rea: u32,
}

/// The IBR reads blocks 0-3 of 2K/ECC8 NAND with BCH T4, so boot blocks
/// must be read and written with T4 too.  Returns what to restore.
unsafe fn enter_block_ecc(sm: &FmiSmInfo, pba: u32) -> Option<EccSave> {
    if pba <= 3 && sm.page_size == NAND_PAGE_2KB && sm.is_nand_ecc8 {
        let save = EccSave {
            tsel: reg_read32(REG_SMCSR) & SMCR_BCH_TSEL,
            rea: reg_read32(REG_SMREAREA_CTL),
        };
        reg_write32(REG_SMCSR, (reg_read32(REG_SMCSR) & !SMCR_BCH_TSEL) | BCH_T4);
        reg_write32(REG_SMREAREA_CTL, (save.rea & !SMRE_REA128_EXT) | 64);
        Some(save)
    } else {
        None
    }
}

unsafe fn leave_block_ecc(save: Option<EccSave>) {
    if let Some(save) = save {
        reg_write32(REG_SMCSR, (reg_read32(REG_SMCSR) & !SMCR_BCH_TSEL) | save.tsel);
        reg_write32(REG_SMREAREA_CTL, save.rea);
    }
}

/// Read a NAND page into buffer
pub unsafe fn sic_sm_pread(chip_sel: i32, pba: i32, page: i32, buff: *mut u8) -> i32 {
    let sm: &FmiSmInfo = &SM0;

    sic_sm_select(chip_sel);
    reg_write32(REG_FMICR, FMI_SM_EN);

    let pba = pba as u32 + sm.lib_start_block;
    let page_no = pba * sm.page_per_block + page as u32;

    let ecc = enter_block_ecc(sm, pba);

    let status = if sm.page_size == NAND_PAGE_2KB {
        // Read redundant area (64 bytes)
        let mut ptr = REG_SMRA_0 as *mut u8;
        fmi_sm_read_ra(sm, page_no, 2048);
        for _ in 0..64 {
            *ptr = (reg_read32(REG_SMDATA) & 0xFF) as u8;
            ptr = ptr.add(1);
        }
        fmi_sm_read_2k(sm, page_no, buff as u32)
    } else if sm.page_size == NAND_PAGE_4KB {
        let mut ptr = REG_SMRA_0 as *mut u8;
        fmi_sm_read_ra(sm, page_no, 4096);
        for _ in 0..128 {
            *ptr = (reg_read32(REG_SMDATA) & 0xFF) as u8;
            ptr = ptr.add(1);
        }
        fmi_sm_read_4k(sm, page_no, buff as u32)
    } else {
        // 512B page
        let mut ptr = REG_SMRA_0 as *mut u8;
        fmi_sm_read_ra_512(sm, page_no, 0);
        for _ in 0..16 {
            *ptr = (reg_read32(REG_SMDATA) & 0xFF) as u8;
            ptr = ptr.add(1);
        }
        fmi_sm_read_512(sm, page_no, buff as u32)
    };

    leave_block_ecc(ecc);
    status
}

// ============================================================================
// SM Program / Erase Operations
// ============================================================================

/// Send the row (page) address cycles, ending with EOA.
unsafe fn fmi_sm_send_row(sm: &FmiSmInfo, page: u32) {
    reg_write32(REG_SMADDR, page & 0xFF); // PA0-PA7
    if !sm.is_multicycle {
        reg_write32(REG_SMADDR, ((page >> 8) & 0xFF) | EOA_SM); // PA8-PA15
    } else {
        reg_write32(REG_SMADDR, (page >> 8) & 0xFF); // PA8-PA15
        reg_write32(REG_SMADDR, ((page >> 16) & 0xFF) | EOA_SM); // PA16-PA17
    }
}

/// Wait for R/B ready and clear its flag before issuing a new command.
unsafe fn fmi_sm_wait_ready() {
    while reg_read32(REG_SMISR) & SMISR_RB0 == 0 {}
    reg_write32(REG_SMISR, SMISR_RB0_IF);
}

/// Issue Read Status; true if the last program/erase passed.
unsafe fn fmi_sm_status_pass() -> bool {
    reg_write32(REG_SMCMD, 0x70);
    reg_read32(REG_SMDATA) & 0x01 == 0
}

/// Spare word 0 for ordinary data pages: byte 0 (bad-block mark) and
/// byte 1 stay 0xFF, bytes 2-3 = 0x00 mark the page as used.
pub const SPARE_DATA: u32 = 0x0000_FFFF;

/// Spare word 0 for page `page` of a loader image of `pages` pages in
/// blocks 0-3.  The IBR follows this sequence (FF 5A nn 00 per page, then
/// the stop marker FF 5A A5 00 on the last one) and ignores a block
/// without it.  Same values as NandWriter's nvtSM_Write_2K().
pub fn spare_boot_mark(page: u32, pages: u32) -> u32 {
    if page + 1 >= pages {
        0x00A5_5AFF
    } else {
        (((0x5A + (page << 8)) & 0xFFFF) << 8) | 0xFF
    }
}

/// Load the spare-area registers for a page program.  The controller
/// inserts ECC parity itself (SMCR_REDUN_AUTO_WEN).
unsafe fn fmi_sm_fill_spare(spare0: u32) {
    let words = (reg_read32(REG_SMREAREA_CTL) & SMRE_REA128_EXT) / 4;
    reg_write32(REG_SMRA_0, spare0);
    for i in 1..words {
        reg_write32(REG_SMRA_0 + i * 4, 0xFFFF_FFFF);
    }
}

/// DMA one page from SDRAM and program it (any page size).
unsafe fn fmi_sm_write_page(sm: &FmiSmInfo, page: u32, saddr: u32, spare0: u32) -> i32 {
    while reg_read32(REG_DMACCSR) & FMI_BUSY != 0 {}
    reg_write32(REG_DMACCSR, reg_read32(REG_DMACCSR) | DMAC_EN);
    reg_write32(REG_DMACSAR, saddr);
    fmi_sm_fill_spare(spare0);

    fmi_sm_wait_ready();
    if sm.page_size == NAND_PAGE_512B {
        reg_write32(REG_SMCMD, 0x00); // pointer to area A
        reg_write32(REG_SMCMD, 0x80); // serial data input
        reg_write32(REG_SMADDR, 0); // CA0-CA7
    } else {
        reg_write32(REG_SMCMD, 0x80); // serial data input
        reg_write32(REG_SMADDR, 0); // CA0-CA7
        reg_write32(REG_SMADDR, 0); // CA8-CA12
    }
    fmi_sm_send_row(sm, page);

    reg_write32(REG_SMISR, SMISR_DMA_IF);
    reg_write32(REG_SMISR, SMISR_ECC_FIELD_IF);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_DWR_EN);
    while reg_read32(REG_SMISR) & SMISR_DMA_IF == 0 {}
    reg_write32(REG_SMISR, SMISR_DMA_IF);

    reg_write32(REG_SMCMD, 0x10); // program confirm
    if fmi_sm_check_rb() == 0 {
        return -1;
    }
    if fmi_sm_status_pass() {
        0
    } else {
        -1
    }
}

/// Program a NAND page from buffer (page_size bytes, word aligned).
/// `spare0` goes to spare word 0: SPARE_DATA or spare_boot_mark().
pub unsafe fn sic_sm_pwrite(chip_sel: i32, pba: i32, page: i32, buff: *const u8, spare0: u32) -> i32 {
    let sm: &FmiSmInfo = &SM0;

    sic_sm_select(chip_sel);
    reg_write32(REG_FMICR, FMI_SM_EN);

    let pba = pba as u32 + sm.lib_start_block;
    let page_no = pba * sm.page_per_block + page as u32;

    let ecc = enter_block_ecc(sm, pba);
    let status = fmi_sm_write_page(sm, page_no, buff as u32, spare0);
    leave_block_ecc(ecc);
    status
}

/// Erase one block.
pub unsafe fn sic_sm_block_erase(chip_sel: i32, pba: i32) -> i32 {
    let sm: &FmiSmInfo = &SM0;

    sic_sm_select(chip_sel);
    reg_write32(REG_FMICR, FMI_SM_EN);

    let pba = pba as u32 + sm.lib_start_block;

    fmi_sm_wait_ready();
    reg_write32(REG_SMCMD, 0x60); // erase setup
    fmi_sm_send_row(sm, pba * sm.page_per_block);
    reg_write32(REG_SMCMD, 0xD0); // erase confirm
    if fmi_sm_check_rb() == 0 {
        return -1;
    }
    if fmi_sm_status_pass() {
        0
    } else {
        -1
    }
}

/// Write a bad-block mark into the spare area of the block's first two
/// pages so check_bad_block_mark() skips it from now on.
pub unsafe fn sic_sm_mark_bad_block(chip_sel: i32, pba: i32) {
    let sm: &FmiSmInfo = &SM0;

    sic_sm_select(chip_sel);
    reg_write32(REG_FMICR, FMI_SM_EN);

    let first = (pba as u32 + sm.lib_start_block) * sm.page_per_block;
    for page in first..first + 2 {
        fmi_sm_wait_ready();
        if sm.page_size == NAND_PAGE_512B {
            reg_write32(REG_SMCMD, 0x50); // pointer to spare area
            reg_write32(REG_SMCMD, 0x80);
            reg_write32(REG_SMADDR, 0);
        } else {
            reg_write32(REG_SMCMD, 0x80);
            reg_write32(REG_SMADDR, sm.page_size & 0xFF);
            reg_write32(REG_SMADDR, (sm.page_size >> 8) & 0xFF);
        }
        fmi_sm_send_row(sm, page);
        for _ in 0..6 {
            reg_write32(REG_SMDATA, 0x00);
        }
        reg_write32(REG_SMCMD, 0x10);
        fmi_sm_check_rb();
        fmi_sm_reset();
    }
}

/// True if the NAND reports itself write protected (status bit 7 low).
pub unsafe fn sic_sm_is_write_protected(chip_sel: i32) -> bool {
    sic_sm_select(chip_sel);
    reg_write32(REG_FMICR, FMI_SM_EN);
    fmi_sm_wait_ready();
    reg_write32(REG_SMCMD, 0x70);
    reg_read32(REG_SMDATA) & 0x80 == 0
}

/// Initialize SM (NAND) interface
pub unsafe fn sic_sm_init() -> i32 {
    if NAND_INITIALIZED {
        return 0;
    }

    // Select NAND control pins
    reg_write32(REG_GPDFUN, reg_read32(REG_GPDFUN) | 0x0003_FC00); // NWR/NRD/RB0/RB1
    reg_write32(REG_GPEFUN, reg_read32(REG_GPEFUN) | 0x00FF_0000); // ALE/CLE/CS0/CS1
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | 0x0600_0000); // CS0/CS1 -> HIGH
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) & !0x0200_0000); // CS0 -> LOW
    reg_write32(REG_FMICR, FMI_SM_EN);

    // Reset SM
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_SM_SWRST);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) & !SMCR_SM_SWRST);

    // Configure SM
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) & !SMCR_PSIZE);
    reg_write32(
        REG_SMCSR,
        reg_read32(REG_SMCSR) | SMCR_ECC_EN | SMCR_ECC_CHK | SMCR_REDUN_AUTO_WEN,
    );
    reg_write32(REG_SMCSR, (reg_read32(REG_SMCSR) & !SMCR_BCH_TSEL) | BCH_T4);

    // Disable mask ECC
    reg_write32(REG_SMREAREA_CTL, reg_read32(REG_SMREAREA_CTL) & !SMRE_MECC);

    // BCH_T4 ECC protect redundant 3 bytes
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_ECC_3B_PROTECT);

    // Chip select
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) & !SMCR_CS0);
    reg_write32(REG_SMCSR, reg_read32(REG_SMCSR) | SMCR_CS1);

    // Set timing (for 192MHz)
    reg_write32(REG_SMTCR, 0x3050B);

    // Clear SM info and read ID
    SM0 = FmiSmInfo::default();
    {
        let sm = &mut SM0;
        if fmi_sm_read_id(sm) < 0 {
            return -1;
        }
        fmi_sm_initial(sm);
    }

    NAND_INITIALIZED = true;
    0
}

/// Initialize FMI device (DMAC + NAND clock)
pub unsafe fn fmi_init_device() {
    // Enable NAND engine clock, disable SD
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) | SIC_CKE | NAND_CKE);
    reg_write32(REG_AHBCLK, reg_read32(REG_AHBCLK) & !SD_CKE);

    // DMAC init
    reg_write32(REG_DMACCSR, 0x0000_0003);
    reg_write32(REG_DMACCSR, 0x0000_0001);

    // Reset FMI
    reg_write32(REG_FMICR, 0x01);
}

// ============================================================================
// Bad Block Management
// ============================================================================

pub unsafe fn check_bad_block_mark(block_no: u32) -> i32 {
    if block_no == 0 {
        return 0; // Block 0 is always valid
    }

    let sm: &FmiSmInfo = &SM0;

    // Check first page
    let mut sector = block_no * sm.page_per_block;
    let status = if sm.page_size == NAND_PAGE_512B {
        fmi_sm_read_ra_512(sm, sector, 0)
    } else {
        fmi_sm_read_ra(sm, sector, sm.page_size)
    };

    if status < 0 {
        sysprintf!("ERROR: CheckBadBlockMark() read fail, for block %d, return %x\n",
            block_no, status as u32);
        return 1;
    }

    if sm.page_size == NAND_PAGE_512B {
        // Check bytes 1 & 6 of spare area
        let data512 = reg_read32(REG_SMDATA) & 0xFF;
        // Skip 4 bytes to get byte 6
        let mut data517: u32 = reg_read32(REG_SMDATA);
        data517 = reg_read32(REG_SMDATA);
        data517 = reg_read32(REG_SMDATA);
        data517 = reg_read32(REG_SMDATA);
        data517 = reg_read32(REG_SMDATA) & 0xFF;

        if data512 == 0xFF && data517 == 0xFF {
            // Check second page
            fmi_sm_reset();
            let status2 = fmi_sm_read_ra_512(sm, sector + 1, 0);
            if status2 < 0 {
                sysprintf!("ERROR: CheckBadBlockMark() read fail, for block %d, return %x\n",
                    block_no, status2 as u32);
                return 1;
            }
            let data512_2 = reg_read32(REG_SMDATA) & 0xFF;
            let mut data517_2: u32 = reg_read32(REG_SMDATA);
            data517_2 = reg_read32(REG_SMDATA);
            data517_2 = reg_read32(REG_SMDATA);
            data517_2 = reg_read32(REG_SMDATA);
            data517_2 = reg_read32(REG_SMDATA) & 0xFF;

            if data512_2 != 0xFF || data517_2 != 0xFF {
                fmi_sm_reset();
                return 1;
            }
        } else {
            fmi_sm_reset();
            return 1;
        }
    } else {
        // 2K/4K: check first byte of spare area
        let block_status = reg_read32(REG_SMDATA) & 0xFF;
        if block_status == 0xFF {
            if sm.is_mlc_nand {
                sector = (block_no + 1) * sm.page_per_block - 1; // Last page
            } else {
                sector += 1; // Second page
            }
            fmi_sm_reset();
            let status2 = fmi_sm_read_ra(sm, sector, sm.page_size);
            if status2 < 0 {
                sysprintf!("ERROR: CheckBadBlockMark() read fail, for block %d, return %x\n",
                    block_no, status2 as u32);
                return 1;
            }
            let block_status2 = reg_read32(REG_SMDATA) & 0xFF;
            if block_status2 != 0xFF {
                fmi_sm_reset();
                return 1;
            }
        } else {
            fmi_sm_reset();
            return 1;
        }
    }

    fmi_sm_reset();
    0
}

/// Get a reference to the global SM0 info
pub unsafe fn get_sm0() -> &'static FmiSmInfo {
    &SM0
}
