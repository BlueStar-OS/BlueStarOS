use super::constant::{RK_DLL_CMD_OUT, RK_RXCLK_NO_INVERTER};

#[derive(Debug, Clone, Copy)]
pub struct EMmcChipConfig {
    pub flags: u32,
    pub hs200_tx_tap: u8,
    pub hs400_tx_tap: u8,
    pub hs400_cmd_tap: u8,
    pub hs400_strbin_tap: u8,
    pub _ddr50_strbin_delay_num: u8,
}

impl EMmcChipConfig {
    pub fn rk3568_config() -> Self {
        Self {
            flags: RK_RXCLK_NO_INVERTER,
            hs200_tx_tap: 16,
            hs400_tx_tap: 8,
            hs400_cmd_tap: 8,
            hs400_strbin_tap: 3,
            _ddr50_strbin_delay_num: 16,
        }
    }

    /// RK3588 配置：不设置 RK_RXCLK_NO_INVERTER，启用 DLL_CMD_OUT
    pub fn rk3588_config() -> Self {
        Self {
            flags: RK_DLL_CMD_OUT,
            hs200_tx_tap: 16,
            hs400_tx_tap: 10,            // DLL_TXCLK_TAPNUM_90_DEGREES = 0xA
            hs400_cmd_tap: 8,            // DLL_CMDOUT_TAPNUM_90_DEGREES
            hs400_strbin_tap: 4,         // DLL_STRBIN_TAPNUM_DEFAULT
            _ddr50_strbin_delay_num: 22, // 0x16
        }
    }
}
