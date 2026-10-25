//! Clock/PLL/SDRAM configuration for N32903
//! Ported from wb_config.c

use n32903_rs::registers::*;

/// Clock source enumeration
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SysClkSrc {
    Ext = 0,
    X32k = 1,
    APll = 2,
    UPll = 3,
}

// Static clock state
static mut EXT_CLOCK_KHZ: u32 = 27_000;
static mut SYS_CLK_SRC: SysClkSrc = SysClkSrc::Ext;
static mut UPLL_KHZ: u32 = 240_000;
static mut APLL_KHZ: u32 = 240_000;
static mut SYS_KHZ: u32 = 120_000;
static mut CPU_KHZ: u32 = 60_000;
static mut HCLK_KHZ: u32 = 60_000;
static mut APB_KHZ: u32 = 0;
static mut CPU_OVER_2X_HCLK: bool = false;

/// Get external clock frequency in KHz
pub unsafe fn sys_get_external_clock() -> u32 {
    if sys_get_chip_version() == b'G' {
        EXT_CLOCK_KHZ = 12_000;
    } else {
        if reg_read32(REG_CHIPCFG) & 0xC == 0x8 {
            EXT_CLOCK_KHZ = 12_000;
        } else {
            EXT_CLOCK_KHZ = 27_000;
        }
    }
    EXT_CLOCK_KHZ
}

/// Get chip version ('G' or 'A')
pub unsafe fn sys_get_chip_version() -> u8 {
    if reg_read32(0xFFFF_3EB4) == 0x5042_3238 {
        b'G'
    } else {
        b'A'
    }
}

// ============================================================================
// PLL Helper Functions
// ============================================================================

/// Get PLL output frequency in KHz
unsafe fn sys_get_pll_output_khz(sys_pll: SysClkSrc, fin_khz: u32) -> u32 {
    let au8_map: [u32; 4] = [1, 2, 2, 4];

    let pll_reg = match sys_pll {
        SysClkSrc::APll => reg_read32(REG_APLLCON),
        SysClkSrc::UPll => reg_read32(REG_UPLLCON),
        _ => return 0,
    };

    if pll_reg & PD != 0 {
        return 0; // PLL powered down
    }

    let nf = (pll_reg & FB_DV) + 2;
    let nr = ((pll_reg & IN_DV) >> 9) + 2;
    let no = au8_map[((pll_reg & OUT_DV) >> 14) as usize];

    fin_khz * nf / nr / no
}

// ============================================================================
// PLL Control Register computation
// ============================================================================

/// Compute PLL control register value for a target frequency
unsafe fn sys_get_pll_ctrl_reg(fin_khz: u32, target_khz: u32) -> u32 {
    let au32_array: [u32; 4] = [1, 2, 2, 4];
    let mut target = target_khz;

    loop {
        for out_dv in 0u32..4 {
            for in_dv in 0u32..32 {
                let nr = 2 * (in_dv + 2);
                if fin_khz / nr < 1000 || fin_khz / nr > 15000 {
                    continue;
                }
                for fb_dv in 0u32..512 {
                    let nf = 2 * (fb_dv + 2);
                    let no = au32_array[out_dv as usize];

                    let vco = fin_khz / nr * nf; // same rounding as wb_config.c
                    if vco < 100_000 || vco > 500_000 {
                        continue;
                    }

                    let result = fin_khz * nf / nr / no;
                    if target == result {
                        return (out_dv << 14) | (in_dv << 9) | fb_dv;
                    }
                }
            }
        }
        if target < 4000 {
            break;
        }
        target -= 4000;
    }
    0
}

// ============================================================================
// DDR SDRAM Initialization (runs from SRAM — DRAM is not usable yet)
// ============================================================================

/// DDR controller init sequence.  Must execute from SRAM because DRAM
/// has not been initialized yet (the code that copies it here handles that).
#[allow(dead_code)] // kept from the BSP; the IBR already initialises DDR
unsafe fn sys_init_ddr() {
    // Configure SDRAM controller
    reg_write32(0xB000_0224, 0x0000_0E6E);
    reg_write32(0xB000_0220, 0x1008_CE6E);
    reg_write32(0xB000_020C, 0x0000_0019);
    // DQS output delay
    reg_write32(0xB000_3030, 0x0000_1010);
    // Precharge all banks
    reg_write32(0xB000_3010, 0x0000_0005);
    reg_write32(0xB000_3004, 0x0000_0021);
    reg_write32(0xB000_3004, 0x0000_0023);
    reg_write32(0xB000_3004, 0x0000_0027);
    while reg_read32(0xB000_3004) & 0x4 != 0 {}
    // Mode register set
    reg_write32(0xB000_301C, 0x0000_1002);
    reg_write32(0xB000_3018, 0x0000_0122);
    reg_write32(0xB000_3004, 0x0000_0027);
    while reg_read32(0xB000_3004) & 0x4 != 0 {}
    // Extended mode register set
    reg_write32(0xB000_3004, 0x0000_002B);
    while reg_read32(0xB000_3004) & 0x8 != 0 {}
    reg_write32(0xB000_3004, 0x0000_002B);
    while reg_read32(0xB000_3004) & 0x8 != 0 {}
    reg_write32(0xB000_3018, 0x0000_0022);
    // Wait 250 cycles
    spin!(250);
    // Normal operation mode
    reg_write32(0xB000_3004, 0x0000_0020);
    // SDRAM size / refresh
    reg_write32(0xB000_3034, 0x00AA_AA00);
    reg_write32(0xB000_3008, 0x0000_80C0);
    reg_write32(0xB000_00A0, 0x0000_0000);
}

// ============================================================================
// SRAM work area (backup buffer for SRAM-relative code execution)
// ============================================================================

const PD_RAM_BASE: u32 = 0xFF00_0000;
const PD_RAM_START: u32 = 0xFF00_1000;
const PD_RAM_SIZE: usize = 0x2000;

/// Backup buffer for SRAM contents while DDR-init or clock-switch code runs there.
static mut TMP_BUF: [u8; PD_RAM_SIZE] = [0u8; PD_RAM_SIZE];

/// Copy `sys_init_ddr` into SRAM and run it there.
/// SRAM is used because DRAM has not been brought up yet — executing
/// DDR-init code from uninitialised DDR would crash immediately.
#[allow(dead_code)]
unsafe fn sys_init_ddr_start() {
    let aic_status = reg_read32(REG_AIC_IMR);
    reg_write32(REG_AIC_MDCR, 0xFFFF_FFFF);

    let vram_base = PD_RAM_BASE as *mut u8;

    // 1. Back up current SRAM contents
    core::ptr::copy_nonoverlapping(vram_base, TMP_BUF.as_mut_ptr(), PD_RAM_SIZE);

    // 2. Copy sys_init_ddr body to SRAM
    core::ptr::copy_nonoverlapping(
        sys_init_ddr as *const u8,
        vram_base,
        PD_RAM_SIZE,
    );

    // 3. Jump to the copy in SRAM
    let func: unsafe extern "C" fn() = core::mem::transmute(vram_base as *const ());
    func();

    // 4. Restore original SRAM
    core::ptr::copy_nonoverlapping(TMP_BUF.as_ptr(), vram_base, PD_RAM_SIZE);

    reg_write32(REG_AIC_MDCR, 0xFFFF_FFFF);
    reg_write32(REG_AIC_MECR, aic_status);
}

// ============================================================================
// Clock Switch (runs from SRAM)
// ============================================================================

/// Clock switch routine - copied to SRAM and executed there.
///
/// Between entering and leaving SDRAM self-refresh nothing may touch SDRAM,
/// so: at most 4 arguments (all passed in registers, none on the stack),
/// no calls, and the dividers are packed into one word:
/// `divs = sys_div | cpu_div << 8 | apb_div << 16`.
#[inline(never)]
unsafe extern "C" fn sys_clock_switch(src_clk: u32, pll_reg: u32, hclk_khz: u32, divs: u32) {
    let sys_div = divs & 0xFF;
    let clkdiv4 = ((divs >> 8) & 0xFF) | (((divs >> 16) & 0xFF) << 8);

    let u32_int_tmp = reg_read32(REG_AIC_IMR);
    reg_write32(REG_AIC_MDCR, 0xFFFF_FFFE);

    // Adjust DDR low-freq mode
    if reg_read32(REG_CHIPCFG) & SDRAMSEL == 0x20 {
        // DDR2
        reg_write32(REG_SDEMR, reg_read32(REG_SDEMR) | DLLEN);
        if hclk_khz < 96_000 {
            reg_write32(REG_SDOPM, reg_read32(REG_SDOPM) | LOWFREQ);
        } else {
            reg_write32(REG_SDOPM, reg_read32(REG_SDOPM) & !LOWFREQ);
        }
    }

    spin!(101);

    // Enter self-refresh
    reg_write32(REG_SDCMD, reg_read32(REG_SDCMD) | AUTOEXSELFREF | REF_CMD);

    if src_clk == SysClkSrc::Ext as u32 {
        reg_write32(REG_CLKDIV0, (reg_read32(REG_CLKDIV0) & !SYSTEM_N0) | sys_div);
        if reg_read32(REG_CHIPCFG) & SDRAMSEL != 0 {
            reg_write32(REG_SDEMR, reg_read32(REG_SDEMR) | DLLEN);
            reg_write32(REG_SDOPM, reg_read32(REG_SDOPM) | LOWFREQ);
        }
        reg_write32(REG_CLKDIV0, reg_read32(REG_CLKDIV0) & !0xFF);
    } else if src_clk == SysClkSrc::UPll as u32 {
        reg_write32(REG_CLKDIV0, reg_read32(REG_CLKDIV0) | 0x02);
        reg_write32(REG_UPLLCON, pll_reg);
        spin!(1000);
        // Wait for UART TX ready
        while reg_read32(REG_UART_FSR + 0x100) & 0x40_0000 == 0 {}
        reg_write8(REG_UART_THR + 0x100, b'\r');
        while reg_read32(REG_UART_FSR + 0x100) & 0x40_0000 == 0 {}
        reg_write32(
            REG_CLKDIV0,
            (reg_read32(REG_CLKDIV0) & !0xF1F) | ((src_clk << 3) | sys_div),
        );
        reg_write32(REG_CLKDIV4, (reg_read32(REG_CLKDIV4) & !0xF0F) | clkdiv4);
        spin!(1000);
    }

    spin!(1000);

    // Exit self-refresh
    reg_write32(REG_SDCMD, reg_read32(REG_SDCMD) & !REF_CMD);

    spin!(1000);

    // Restore interrupt mask
    reg_write32(REG_AIC_MECR, u32_int_tmp);
}

/// Copy clock switch code to SRAM and execute
unsafe fn sys_clock_switch_start(
    src_clk: SysClkSrc,
    pll_reg: u32,
    hclk: u32,
    sys_div: u32,
    cpu_div: u32,
    apb_div: u32,
) {
    let aic_status = reg_read32(REG_AIC_IMR);
    reg_write32(REG_AIC_MDCR, 0xFFFF_FFFF);

    let vram_base = PD_RAM_BASE as u32;
    // Backup VRAM
    core::ptr::copy_nonoverlapping(
        (vram_base | 0x8000_0000) as *const u8,
        TMP_BUF.as_mut_ptr(),
        PD_RAM_SIZE,
    );
    // Copy sys_clock_switch to RAM
    core::ptr::copy_nonoverlapping(
        (sys_clock_switch as *const u8)
            .offset(-((PD_RAM_START - PD_RAM_BASE) as isize)),
        (vram_base | 0x8000_0000) as *mut u8,
        PD_RAM_SIZE,
    );

    let wb_func: unsafe extern "C" fn(u32, u32, u32, u32) =
        core::mem::transmute(PD_RAM_START as *const ());
    wb_func(
        src_clk as u32,
        pll_reg,
        hclk,
        (sys_div & 0xFF) | (cpu_div & 0xFF) << 8 | (apb_div & 0xFF) << 16,
    );

    // Restore VRAM
    core::ptr::copy_nonoverlapping(
        TMP_BUF.as_ptr(),
        (vram_base | 0x8000_0000) as *mut u8,
        PD_RAM_SIZE,
    );

    reg_write32(REG_AIC_MDCR, 0xFFFF_FFFF);
    reg_write32(REG_AIC_MECR, aic_status);
}

// ============================================================================
// Main Clock Configuration
// ============================================================================

/// Set system clock (main PLL configuration entry point)
pub unsafe fn sys_set_system_clock(
    src_clk: SysClkSrc,
    pll_khz: u32,
    sys_khz: u32,
    cpu_khz: u32,
    hclk_khz: u32,
    apb_khz: u32,
) -> i32 {
    EXT_CLOCK_KHZ = sys_get_external_clock();

    let mut sys_div = 0u32;
    let mut cpu_div = 0u32;
    let mut apb_div = 0u32;

    if src_clk != SysClkSrc::Ext {
        // Validate clock constraints
        if sys_khz > pll_khz {
            return -1;
        }
        if cpu_khz > sys_khz {
            return -1;
        }
        if hclk_khz > sys_khz {
            return -1;
        }
        if apb_khz > hclk_khz {
            return -1;
        }

        CPU_OVER_2X_HCLK = cpu_khz > hclk_khz * 2;

        sys_div = pll_khz / sys_khz - 1;
        if pll_khz % sys_khz != 0 {
            sys_div += 1;
        }
        SYS_KHZ = pll_khz / (sys_div + 1);

        if CPU_OVER_2X_HCLK {
            HCLK_KHZ = SYS_KHZ / 6;

            cpu_div = SYS_KHZ / cpu_khz - 1;
            if SYS_KHZ % cpu_khz != 0 {
                cpu_div += 1;
            }
            if cpu_div == 0 {
                cpu_div = 1;
            }
            if cpu_div > 5 {
                cpu_div = 5;
            }
            CPU_KHZ = SYS_KHZ / (cpu_div + 1);

            let hclk_div = 6u32;
            let apb_idx = SYS_KHZ / apb_khz;
            apb_div = apb_idx / hclk_div;
            if apb_div > 0 {
                apb_div -= 1;
            }
            APB_KHZ = SYS_KHZ / ((apb_div + 1) * hclk_div);
        } else {
            HCLK_KHZ = SYS_KHZ / 2;

            cpu_div = SYS_KHZ / cpu_khz - 1;
            if SYS_KHZ % cpu_khz != 0 {
                cpu_div += 1;
            }
            if cpu_div > 1 {
                return -1;
            }
            CPU_KHZ = SYS_KHZ / (cpu_div + 1);

            let hclk_div = if cpu_div > 2 { cpu_div } else { 2u32 };
            let apb_idx = SYS_KHZ / apb_khz;
            apb_div = apb_idx / hclk_div - 1;
            APB_KHZ = SYS_KHZ / ((apb_div + 1) * hclk_div);
        }
    }

    // Configure PLL register
    let pll_reg = match src_clk {
        SysClkSrc::Ext => {
            HCLK_KHZ = 27_000;
            CPU_OVER_2X_HCLK = false;
            sys_div = 1;
            cpu_div = 1;
            apb_div = 1;
            0
        }
        SysClkSrc::APll => {
            APLL_KHZ = pll_khz;
            sys_get_pll_ctrl_reg(EXT_CLOCK_KHZ, APLL_KHZ)
        }
        SysClkSrc::UPll => {
            UPLL_KHZ = pll_khz;
            sys_get_pll_ctrl_reg(EXT_CLOCK_KHZ, UPLL_KHZ)
        }
        _ => return -1,
    };

    // One switch with the PLL value and the dividers together, as
    // sysSetSystemClock() does.  (Switching with pll_reg = 0 first would
    // turn the PLL off and run the CPU from it.)
    SYS_CLK_SRC = src_clk;
    sys_clock_switch_start(src_clk, pll_reg, HCLK_KHZ, sys_div, cpu_div, apb_div);

    0
}

/// Get current system clock frequencies
pub unsafe fn sys_get_system_clock(
    src_clk: &mut SysClkSrc,
    pll_khz: &mut u32,
    sys_khz: &mut u32,
    cpu_khz: &mut u32,
    hclk_khz: &mut u32,
    apb_khz: &mut u32,
) {
    *src_clk = SYS_CLK_SRC;
    match SYS_CLK_SRC {
        SysClkSrc::UPll => *pll_khz = UPLL_KHZ,
        SysClkSrc::APll => *pll_khz = APLL_KHZ,
        _ => *pll_khz = 0,
    }
    *sys_khz = SYS_KHZ;
    *cpu_khz = CPU_KHZ;
    *hclk_khz = HCLK_KHZ;
    *apb_khz = APB_KHZ;
}

// ============================================================================
// initClock - Initial system clock setup, ported from NandLoader.c
// ============================================================================

const E_CLKSKEW: u32 = 0x0088_8800;

pub unsafe fn init_clock() {
    // DDR is already running: the IBR set it up from the boot header before
    // loading us into it.  Don't re-run sys_init_ddr here (the original
    // NandLoader doesn't either) - redoing the power-up sequence on live
    // SDRAM hangs in the precharge-all wait.

    let ext_freq = sys_get_external_clock();

    reg_write32(REG_DQSODS, 0x1010);
    reg_write32(REG_CKDQSDS, E_CLKSKEW);

    if ext_freq == 12000 {
        reg_write32(REG_SDREF, 0x805A);
    } else {
        reg_write32(REG_SDREF, 0x80C0);
    }

    // DDR grade-6
    reg_write32(REG_SDTIME, 0x094E_7425);
    reg_write32(REG_SDMR, 0x22); // CAS Latency = 2

    // UPLL 200 MHz (12 MHz XIN: NR = 3, NF = 50, NO = 1; VCO 200 MHz).
    // CPU = SYS = 200 MHz, HCLK = SYS / 2 = 100 MHz (DDR/AHB — above 96 MHz
    // the switch runs DDR with its DLL on), APB = 50 MHz. UART and timers
    // run from XIN, so their rates do not change. NAND timing (SMTCR) is the
    // BSP's 192 MHz setting.
    sys_set_system_clock(SysClkSrc::UPll, 200_000, 200_000, 200_000, 200_000 / 2, 200_000 / 4);
}
