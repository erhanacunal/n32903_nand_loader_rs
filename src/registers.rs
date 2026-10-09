//! Register definitions for Nuvoton N32903 (W55FA93)
//! Ported from W55FA93_reg.h

#![allow(dead_code)]

// ============================================================================
// Base Addresses
// ============================================================================
pub const SYS_BA: u32 = 0xB000_0000; // System Manager Control
pub const GCR_BA: u32 = SYS_BA;
pub const CLK_BA: u32 = 0xB000_0200; // Clock Controller
pub const SDRAM_BA: u32 = 0xB000_3000; // SDRAM Interface
pub const FMI_BA: u32 = 0xB100_6800; // Flash Memory Interface
pub const DMAC_BA: u32 = 0xB100_6000; // DMA Control
pub const SPU_BA: u32 = 0xB100_0000; // SPU Controller
pub const AIC_BA: u32 = 0xB800_0000; // Interrupt Controller
pub const GPIO_BA: u32 = 0xB800_1000; // GPIO Control
pub const UART_BA: u32 = 0xB800_8000; // UART Control
pub const TMR_BA: u32 = 0xB800_2000; // Timer Control
pub const RTC_BA: u32 = 0xB800_3000; // RTC Control

// ============================================================================
// System Manager (GCR) Registers
// ============================================================================
pub const REG_CHIPCFG: u32 = GCR_BA + 0x04;
pub const REG_AHBCTL: u32 = GCR_BA + 0x10;
pub const REG_AHBIPRST: u32 = GCR_BA + 0x14;
pub const REG_APBIPRST: u32 = GCR_BA + 0x18;
pub const REG_MISCR: u32 = GCR_BA + 0x20;
pub const REG_GPAFUN: u32 = GCR_BA + 0x80;
pub const REG_GPBFUN: u32 = GCR_BA + 0x84;
pub const REG_GPCFUN: u32 = GCR_BA + 0x88;
pub const REG_GPDFUN: u32 = GCR_BA + 0x8C;
pub const REG_GPEFUN: u32 = GCR_BA + 0x90;

// GPIO port A
pub const REG_GPIOA_OMD: u32 = GPIO_BA + 0x00; // output mode enable
pub const REG_GPIOA_DOUT: u32 = GPIO_BA + 0x08; // data output
pub const REG_MISCPCR: u32 = GCR_BA + 0xA0;

// CHIPCFG bit fields
pub const SDRAMSEL: u32 = 0x30;
// CHIPCFG CLK_SRC bit
pub const CLK_SRC: u32 = 0x40;

// ============================================================================
// Clock Controller Registers
// ============================================================================
pub const REG_PWRCON: u32 = CLK_BA + 0x00;
pub const REG_AHBCLK: u32 = CLK_BA + 0x04;
pub const REG_APBCLK: u32 = CLK_BA + 0x08;
pub const REG_CLKDIV0: u32 = CLK_BA + 0x0C;
pub const REG_CLKDIV4: u32 = CLK_BA + 0x1C;
pub const REG_APLLCON: u32 = CLK_BA + 0x20;
pub const REG_UPLLCON: u32 = CLK_BA + 0x24;

// PWRCON bit fields
pub const UP2HCLK3X: u32 = 0x20;

// AHBCLK bit fields
pub const ADO_CKE: u32 = 0x4000_0000;
pub const SPU_CKE: u32 = 0x0200_0000;
pub const HCLK4_CKE: u32 = 0x0100_0000;
pub const SD_CKE: u32 = 0x0080_0000;
pub const NAND_CKE: u32 = 0x0040_0000;
pub const SIC_CKE: u32 = 0x0020_0000;

// CLKDIV0 bit fields
pub const SYSTEM_N1: u32 = 0x0000_0F00;
pub const SYSTEM_S: u32 = 0x0000_0018;
pub const SYSTEM_N0: u32 = 0x0000_0007;

// CLKDIV4 bit fields
pub const APB_N: u32 = 0x0000_0F00;
pub const CPU_N: u32 = 0x0000_000F;

// PLL bit fields
pub const PD: u32 = 0x0001_0000;
pub const OUT_DV: u32 = 0x0000_C000;
pub const IN_DV: u32 = 0x0000_3E00;
pub const FB_DV: u32 = 0x0000_01FF;

// ============================================================================
// SDRAM Controller Registers
// ============================================================================
pub const REG_SDOPM: u32 = SDRAM_BA + 0x00;
pub const REG_SDCMD: u32 = SDRAM_BA + 0x04;
pub const REG_SDREF: u32 = SDRAM_BA + 0x08;
pub const REG_SDMR: u32 = SDRAM_BA + 0x18;
pub const REG_SDEMR: u32 = SDRAM_BA + 0x1C;
pub const REG_SDTIME: u32 = SDRAM_BA + 0x28;
pub const REG_DQSODS: u32 = SDRAM_BA + 0x30;
pub const REG_CKDQSDS: u32 = SDRAM_BA + 0x34;

// SDCMD bit fields
pub const AUTOEXSELFREF: u32 = 0x20;
pub const REF_CMD: u32 = 0x08;

// SDEMR bit fields
pub const DLLEN: u32 = 0x01;

// SDOPM bit fields
pub const LOWFREQ: u32 = 0x0004_0000;
pub const DRAM_EN: u32 = 0x0002;

// SDREF bit fields
pub const REF_EN: u32 = 0x8000;
pub const REFRATE: u32 = 0x7FFF;

// ============================================================================
// FMI (Flash Memory Interface) Registers
// ============================================================================
pub const REG_FMICR: u32 = FMI_BA + 0x000;
pub const REG_SMCSR: u32 = FMI_BA + 0x0A0;
pub const REG_SMTCR: u32 = FMI_BA + 0x0A4;
pub const REG_SMIER: u32 = FMI_BA + 0x0A8;
pub const REG_SMISR: u32 = FMI_BA + 0x0AC;
pub const REG_SMCMD: u32 = FMI_BA + 0x0B0;
pub const REG_SMADDR: u32 = FMI_BA + 0x0B4;
pub const REG_SMDATA: u32 = FMI_BA + 0x0B8;
pub const REG_SMREAREA_CTL: u32 = FMI_BA + 0x0BC;
pub const REG_SM_ECC_ST0: u32 = FMI_BA + 0x0D0;
pub const REG_SM_ECC_ST1: u32 = FMI_BA + 0x0D4;
pub const REG_SM_ECC_ST2: u32 = FMI_BA + 0x0D8;
pub const REG_SM_ECC_ST3: u32 = FMI_BA + 0x0DC;
pub const REG_BCH_ECC_ADDR0: u32 = FMI_BA + 0x100;
pub const REG_BCH_ECC_DATA0: u32 = FMI_BA + 0x160;
pub const REG_SMRA_0: u32 = FMI_BA + 0x200;

// FMICR
pub const FMI_SM_EN: u32 = 0x08;
pub const FMI_SWRST: u32 = 0x01;

// SMCSR bit fields
pub const SMCR_CS1: u32 = 0x0400_0000;
pub const SMCR_CS0: u32 = 0x0200_0000;
pub const SMCR_ECC_EN: u32 = 0x0080_0000;
pub const SMCR_BCH_TSEL: u32 = 0x0078_0000;
pub const SMCR_PSIZE: u32 = 0x0003_0000;
pub const SMCR_SRAM_INIT: u32 = 0x0000_0200;
pub const SMCR_ECC_3B_PROTECT: u32 = 0x0000_0100;
pub const SMCR_ECC_CHK: u32 = 0x0000_0080;
pub const SMCR_REDUN_AUTO_WEN: u32 = 0x0000_0010;
pub const SMCR_REDUN_REN: u32 = 0x0000_0008;
pub const SMCR_DWR_EN: u32 = 0x0000_0004;
pub const SMCR_DRD_EN: u32 = 0x0000_0002;
pub const SMCR_SM_SWRST: u32 = 0x0000_0001;

// BCH T-sel values
pub const BCH_T15: u32 = 0x0040_0000;
pub const BCH_T12: u32 = 0x0020_0000;
pub const BCH_T8: u32 = 0x0010_0000;
pub const BCH_T4: u32 = 0x0008_0000;

// Page size values
pub const PSIZE_8K: u32 = 0x0003_0000;
pub const PSIZE_4K: u32 = 0x0002_0000;
pub const PSIZE_2K: u32 = 0x0001_0000;
// PSIZE_512 = 0

// SMISR bit fields
pub const SMISR_RB0: u32 = 0x0004_0000;
pub const SMISR_RB1: u32 = 0x0008_0000;
pub const SMISR_RB0_IF: u32 = 0x0000_0400;
pub const SMISR_RB1_IF: u32 = 0x0000_0800;
pub const SMISR_ECC_FIELD_IF: u32 = 0x0000_0004;
pub const SMISR_DMA_IF: u32 = 0x0000_0001;

// SMADDR
pub const EOA_SM: u32 = 0x8000_0000;

// SMREAREA_CTL bit fields
pub const SMRE_MECC: u32 = 0xFFFF_0000;
pub const SMRE_REA128_EXT: u32 = 0x0000_01FF;

// ============================================================================
// DMAC Registers
// ============================================================================
pub const REG_DMACCSR: u32 = DMAC_BA + 0x400;
pub const REG_DMACSAR: u32 = DMAC_BA + 0x408;
pub const REG_DMACBCR: u32 = DMAC_BA + 0x40C;

// DMACCSR bit fields
pub const FMI_BUSY: u32 = 0x0000_0200;
pub const DMAC_EN: u32 = 0x0000_0001;

// ============================================================================
// SPU Registers
// ============================================================================
pub const REG_SPU_CTRL: u32 = SPU_BA + 0x00;
pub const REG_SPU_DAC_PAR: u32 = SPU_BA + 0x04;
pub const REG_SPU_DAC_VOL: u32 = SPU_BA + 0x08;

// SPU_CTRL
pub const SPURST: u32 = 0x0001_0000;

// ============================================================================
// AIC Registers
// ============================================================================
pub const REG_AIC_IMR: u32 = AIC_BA + 0x114;
pub const REG_AIC_MECR: u32 = AIC_BA + 0x120;
pub const REG_AIC_MDCR: u32 = AIC_BA + 0x124;

// ============================================================================
// UART Registers
// ============================================================================
pub const REG_UART_RBR: u32 = UART_BA + 0x00;
pub const REG_UART_THR: u32 = UART_BA + 0x00;
pub const REG_UART_IER: u32 = UART_BA + 0x04;
pub const REG_UART_FCR: u32 = UART_BA + 0x08;
pub const REG_UART_LCR: u32 = UART_BA + 0x0C;
pub const REG_UART_FSR: u32 = UART_BA + 0x18;
pub const REG_UART_ISR: u32 = UART_BA + 0x1C;
pub const REG_UART_TOR: u32 = UART_BA + 0x20;
pub const REG_UART_BAUD: u32 = UART_BA + 0x24;

// UART FSR bit fields
pub const RX_OVER_IF: u32 = 0x0000_0001;
pub const RX_EMPTY: u32 = 0x0000_4000;
pub const TX_EMPTY: u32 = 0x0040_0000;
pub const TE_FLAG: u32 = 0x1000_0000; // FIFO and shift register empty

// UART ISR bit fields
pub const THRE_IF: u32 = 0x0000_0002;
pub const THRE_IEN: u32 = 0x0000_0002;

// ============================================================================
// Timer Registers
// ============================================================================
pub const REG_TCSR0: u32 = TMR_BA + 0x00;
pub const REG_TICR0: u32 = TMR_BA + 0x08;
pub const REG_TDR0: u32 = TMR_BA + 0x10;
pub const REG_TISR: u32 = TMR_BA + 0x18;

// TCSR bit fields
pub const TCSR_CEN: u32 = 0x4000_0000;
pub const TCSR_MODE_PERIODIC: u32 = 0x0800_0000;
pub const TCSR_CRST: u32 = 0x0400_0000;

// TISR bit fields
pub const TISR_TIF0: u32 = 0x0000_0001;

// APBCLK bit fields
pub const TMR0_CKE: u32 = 0x0000_0100;

// ============================================================================
// RTC Registers
// ============================================================================
pub const AER: u32 = RTC_BA + 0x04;
pub const PWRON: u32 = RTC_BA + 0x34;
pub const RIIR: u32 = RTC_BA + 0x2C;

// ============================================================================
// MMIO Helper Functions
// ============================================================================

/// Write 32-bit value to MMIO register
#[inline(always)]
pub unsafe fn reg_write32(addr: u32, value: u32) {
    core::ptr::write_volatile(addr as *mut u32, value);
}

/// Read 32-bit value from MMIO register
#[inline(always)]
pub unsafe fn reg_read32(addr: u32) -> u32 {
    core::ptr::read_volatile(addr as *const u32)
}

/// Write 16-bit value to MMIO register
#[inline(always)]
pub unsafe fn reg_write16(addr: u32, value: u16) {
    core::ptr::write_volatile(addr as *mut u16, value);
}

/// Read 16-bit value from MMIO register
#[inline(always)]
pub unsafe fn reg_read16(addr: u32) -> u16 {
    core::ptr::read_volatile(addr as *const u16)
}

/// Write 8-bit value to MMIO register
#[inline(always)]
pub unsafe fn reg_write8(addr: u32, value: u8) {
    core::ptr::write_volatile(addr as *mut u8, value);
}

/// Read 8-bit value from MMIO register
#[inline(always)]
pub unsafe fn reg_read8(addr: u32) -> u8 {
    core::ptr::read_volatile(addr as *const u8)
}
