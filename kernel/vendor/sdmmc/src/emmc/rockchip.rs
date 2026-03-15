use super::{EMmcHost, constant::*};
use crate::{
    delay_us,
    emmc::{aux::dll_lock_wo_tmout, clock::emmc_set_clk, config::EMmcChipConfig},
    err::SdError,
};
use log::{debug, info};

impl EMmcHost {
    // Rockchip EMMC设置时钟函数
    pub fn rockchip_emmc_set_clock(&mut self, freq: u32) -> Result<(), SdError> {
        // wait for command and data inhibit to be cleared
        let mut timeout = 20;
        while (self.read_reg(EMMC_PRESENT_STATE) & (EMMC_CMD_INHIBIT | EMMC_DATA_INHIBIT)) != 0 {
            if timeout == 0 {
                debug!("Timeout waiting for cmd & data inhibit");
                return Err(SdError::Timeout);
            }
            timeout -= 1;
            delay_us(1000);
        }

        // first disable the clock
        self.write_reg16(EMMC_CLOCK_CONTROL, 0x0000);

        if freq == 0 {
            return Ok(());
        }

        // Rockchip 平台识别模式只支持 375KHz
        let freq = if freq <= 400000 { 375000 } else { freq };

        // 计算输入时钟
        let input_clk = emmc_set_clk(freq as u64).unwrap() as u32;
        info!("input_clk: {}", input_clk);

        let mut div = 0;
        let mut clk = 0u16;
        let sdhci_version = self.read_reg16(EMMC_HOST_CNTRL_VER);

        if (sdhci_version & 0xFF) >= EMMC_SPEC_300 {
            let caps2 = self.read_reg(EMMC_CAPABILITIES2);
            let clk_mul = (caps2 & EMMC_CLOCK_MUL_MASK) >> EMMC_CLOCK_MUL_SHIFT;

            info!("EMMC Clock Mul: {}", clk_mul);

            // Check if the Host Controller supports Programmable Clock Mode.
            if clk_mul != 0 {
                for i in 1..=1024 {
                    if (input_clk / i) <= freq {
                        div = i;
                        break;
                    }
                }
                // Set Programmable Clock Mode in the Clock Control register.
                clk = EMMC_PROG_CLOCK_MODE;
                div -= 1;
            } else {
                // Version 3.00 divisors must be a multiple of 2.
                if input_clk <= freq {
                    div = 1;
                } else {
                    for i in (2..=2046).step_by(2) {
                        if (input_clk / i) <= freq {
                            div = i;
                            break;
                        }
                    }
                }
                div >>= 1;
            }
        } else {
            // Version 2.00 divisors must be a power of 2.
            let mut i = 1;
            while i < 256 && (input_clk / i) > freq {
                i *= 2;
            }
            div = i >> 1;
        }

        info!("EMMC Clock Divisor: 0x{:x}", div);

        clk |= ((div as u16) & 0xFF) << EMMC_DIVIDER_SHIFT;
        clk |= (((div as u16) & 0x300) >> 8) << EMMC_DIVIDER_HI_SHIFT;

        self.write_reg16(EMMC_CLOCK_CONTROL, clk);
        self.enable_card_clock(clk)?;

        Ok(())
    }

    pub fn enable_card_clock(&mut self, mut clk: u16) -> Result<(), SdError> {
        clk |= EMMC_CLOCK_INT_EN;
        clk &= !EMMC_CLOCK_INT_STABLE;
        self.write_reg16(EMMC_CLOCK_CONTROL, clk);

        let mut timeout = 20;
        while (self.read_reg16(EMMC_CLOCK_CONTROL) & EMMC_CLOCK_INT_STABLE) == 0 {
            timeout -= 1;
            delay_us(1000);
            if timeout == 0 {
                info!("Internal clock never stabilised.");
                return Err(SdError::Timeout);
            }
        }

        self.write_reg16(EMMC_CLOCK_CONTROL, clk | EMMC_CLOCK_CARD_EN);

        debug!(
            "EMMC Clock Control: {:#x}",
            self.read_reg16(EMMC_CLOCK_CONTROL)
        );

        Ok(())
    }

    pub fn is_clock_stable(&self) -> bool {
        let clock_ctrl = self.read_reg16(EMMC_CLOCK_CONTROL);
        (clock_ctrl & EMMC_CLOCK_INT_STABLE) != 0
    }

    pub fn sdhci_set_power(&mut self, power: u32) -> Result<(), SdError> {
        let mut pwr: u8 = 0;

        if power != 0xFFFF {
            match 1 << power {
                MMC_VDD_165_195 => {
                    pwr = EMMC_POWER_180;
                }
                MMC_VDD_29_30 | MMC_VDD_30_31 => {
                    pwr = EMMC_POWER_300;
                }
                MMC_VDD_32_33 | MMC_VDD_33_34 => {
                    pwr = EMMC_POWER_330;
                }
                _ => {}
            }
        }

        if pwr == 0 {
            self.write_reg8(EMMC_POWER_CTRL, 0);
            return Ok(());
        }

        pwr |= EMMC_POWER_ON;
        self.write_reg8(EMMC_POWER_CTRL, pwr);

        info!("EMMC Power Control: {:#x}", self.read_reg8(EMMC_POWER_CTRL));

        // Small delay for power to stabilize
        delay_us(10000);

        Ok(())
    }

    // DWCMSHC SDHCI EMMC设置时钟
    pub fn dwcmshc_sdhci_emmc_set_clock(&mut self, freq: u32) -> Result<(), SdError> {
        let mut timeout = 500;
        let timing = self.card.as_ref().unwrap().timing;
        let data = EMmcChipConfig::rk3588_config();

        self.rockchip_emmc_set_clock(freq)?;

        // 保存分频器设置，然后禁用时钟输出以配置 DLL
        let clk_saved = self.read_reg16(EMMC_CLOCK_CONTROL);
        // 只保留分频器位 (bit 6-15)，清除使能位
        let clk_div = clk_saved & !( EMMC_CLOCK_INT_EN | EMMC_CLOCK_INT_STABLE | EMMC_CLOCK_CARD_EN);
        self.write_reg16(EMMC_CLOCK_CONTROL, 0);

        debug!(
            "EMMC Clock Control: {:#x}, saved div: {:#x}",
            self.read_reg16(EMMC_CLOCK_CONTROL),
            clk_div
        );

        // Disable cmd conflict check and enable internal clock gate
        let mut extra = self.read_reg(DWCMSHC_HOST_CTRL3);
        extra &= !0x1;
        extra |= 1 << 4;
        self.write_reg(DWCMSHC_HOST_CTRL3, extra);

        // DLL配置基于频率 (Linux 用 52MHz 作为分界线)
        if freq > 52_000_000 {
            // Reset DLL
            self.write_reg(DWCMSHC_EMMC_DLL_CTRL, DWCMSHC_EMMC_DLL_CTRL_RESET);
            delay_us(1);
            self.write_reg(DWCMSHC_EMMC_DLL_CTRL, 0);

            // RK3588: RXCLK 只设 DLYENA，不设 NO_INVERTER 和 ORI_GATE
            // RK3568: RXCLK 设 DLYENA | NO_INVERTER
            let mut extra = DWCMSHC_EMMC_DLL_DLYENA;
            if (data.flags & RK_RXCLK_NO_INVERTER) != 0 {
                extra |= DLL_RXCLK_NO_INVERTER;
            }
            self.write_reg(DWCMSHC_EMMC_DLL_RXCLK, extra);

            // Init DLL settings
            extra = DWCMSHC_EMMC_DLL_START_DEFAULT << DWCMSHC_EMMC_DLL_START_POINT
                | DWCMSHC_EMMC_DLL_INC_VALUE << DWCMSHC_EMMC_DLL_INC
                | DWCMSHC_EMMC_DLL_START;
            self.write_reg(DWCMSHC_EMMC_DLL_CTRL, extra);

            // Wait for DLL lock
            loop {
                if timeout <= 0 {
                    info!("Timeout waiting for DLL to be ready");
                    return Err(SdError::Timeout);
                }

                if dll_lock_wo_tmout(self.read_reg(DWCMSHC_EMMC_DLL_STATUS0)) {
                    break;
                }

                delay_us(1000);
                timeout -= 1;
            }

            // ATCTRL: tune clock stop en + pre/post change delay
            extra = 0x1 << 16 | 0x3 << 17 | 0x3 << 19;
            self.write_reg(DWCMSHC_EMMC_ATCTRL, extra);

            let mut txclk_tapnum = data.hs200_tx_tap;

            // RK3588 HS400: 配置 CMDOUT 并覆盖 txclk tap
            if (data.flags & RK_DLL_CMD_OUT) != 0
                && (timing == MMC_TIMING_MMC_HS400 || timing == MMC_TIMING_MMC_HS400ES)
            {
                txclk_tapnum = data.hs400_tx_tap;

                extra = DLL_CMDOUT_SRC_CLK_NEG
                    | DLL_CMDOUT_EN_SRC_CLK_NEG
                    | DWCMSHC_EMMC_DLL_DLYENA
                    | (data.hs400_cmd_tap as u32)
                    | DLL_CMDOUT_TAPNUM_FROM_SW;

                self.write_reg(DECMSHC_EMMC_DLL_CMDOUT, extra);
            }

            // TXCLK
            extra = DWCMSHC_EMMC_DLL_DLYENA
                | DLL_TXCLK_TAPNUM_FROM_SW
                | DLL_RXCLK_NO_INVERTER  // Linux: DLL_RXCLK_NO_INVERTER << DWCMSHC_EMMC_DLL_RXCLK_SRCSEL (bit 29)
                | txclk_tapnum as u32;
            self.write_reg(DWCMSHC_EMMC_DLL_TXCLK, extra);

            // STRBIN
            extra = DWCMSHC_EMMC_DLL_DLYENA
                | DLL_STRBIN_TAPNUM_DEFAULT
                | DLL_STRBIN_TAPNUM_FROM_SW;
            self.write_reg(DWCMSHC_EMMC_DLL_STRBIN, extra);
        } else {
            // Bypass DLL: 低速模式 (<=52MHz)
            self.write_reg(
                DWCMSHC_EMMC_DLL_CTRL,
                DWCMSHC_EMMC_DLL_BYPASS | DWCMSHC_EMMC_DLL_START,
            );
            self.write_reg(DWCMSHC_EMMC_DLL_RXCLK, DLL_RXCLK_ORI_GATE);
            self.write_reg(DWCMSHC_EMMC_DLL_TXCLK, 0);
            self.write_reg(DECMSHC_EMMC_DLL_CMDOUT, 0);

            // Enhanced strobe PHY 参数 (hs400es 切换前需要)
            let extra = DWCMSHC_EMMC_DLL_DLYENA
                | DLL_STRBIN_DELAY_NUM_SEL
                | ((data._ddr50_strbin_delay_num as u32) << DLL_STRBIN_DELAY_NUM_OFFSET);
            self.write_reg(DWCMSHC_EMMC_DLL_STRBIN, extra);
        }

        // 带分频器重新使能时钟
        self.enable_card_clock(clk_div)?;

        info!("Clock {:#x}", self.read_reg16(EMMC_CLOCK_CONTROL));

        Ok(())
    }

    pub fn sdhci_set_uhs_signaling(&self) {
        let timing = self.card.as_ref().unwrap().timing;

        let mut ctrl_2 = self.read_reg16(EMMC_HOST_CTRL2);
        ctrl_2 &= !MMC_CTRL_UHS_MASK;

        if (timing != MMC_TIMING_LEGACY)
            && (timing != MMC_TIMING_MMC_HS)
            && (timing != MMC_TIMING_SD_HS)
        {
            ctrl_2 |= MMC_CTRL_VDD_180;
        }

        if (timing == MMC_TIMING_MMC_HS200) || (timing == MMC_TIMING_UHS_SDR104) {
            ctrl_2 |= MMC_CTRL_UHS_SDR104 | MMC_CTRL_DRV_TYPE_A;
        } else if timing == MMC_TIMING_UHS_SDR12 {
            ctrl_2 |= MMC_CTRL_UHS_SDR12;
        } else if timing == MMC_TIMING_UHS_SDR25 {
            ctrl_2 |= MMC_CTRL_UHS_SDR25;
        } else if (timing == MMC_TIMING_UHS_SDR50) || (timing == MMC_TIMING_MMC_HS) {
            ctrl_2 |= MMC_CTRL_UHS_SDR50;
        } else if (timing == MMC_TIMING_UHS_DDR50) || (timing == MMC_TIMING_MMC_DDR52) {
            ctrl_2 |= MMC_CTRL_UHS_DDR50;
        } else if timing == MMC_TIMING_MMC_HS400 || timing == MMC_TIMING_MMC_HS400ES {
            ctrl_2 |= MMC_CTRL_HS400 | MMC_CTRL_DRV_TYPE_A;
        }

        debug!("EMMC Host Control 2: {:#x}", ctrl_2);

        self.write_reg16(EMMC_HOST_CTRL2, ctrl_2);
    }

    pub fn sdhci_set_ios(&mut self) {
        let (card_clock, bus_width, timing) = {
            let card = self.card.as_ref().unwrap();
            (card.clock, card.bus_width, card.timing)
        };

        debug!(
            "card_clock: {}, bus_width: {}, timing: {}",
            card_clock, bus_width, timing
        );

        self.dwcmshc_sdhci_emmc_set_clock(card_clock).unwrap();

        /* Set bus width */
        let mut ctrl = self.read_reg8(EMMC_HOST_CTRL1);
        if bus_width == 8 {
            ctrl &= !EMMC_CTRL_4BITBUS;
            if self.sdhci_get_version() >= EMMC_SPEC_300
                || (self.quirks & SDHCI_QUIRK_USE_WIDE8) != 0
            {
                ctrl |= EMMC_CTRL_8BITBUS;
            }
        } else {
            if self.sdhci_get_version() >= EMMC_SPEC_300
                || (self.quirks & SDHCI_QUIRK_USE_WIDE8) != 0
            {
                ctrl &= !EMMC_CTRL_8BITBUS;
            }
            if bus_width == 4 {
                ctrl |= EMMC_CTRL_4BITBUS;
            } else {
                ctrl &= !EMMC_CTRL_4BITBUS;
            }
        }

        if (timing != MMC_TIMING_LEGACY) && (self.quirks & SDHCI_QUIRK_NO_HISPD_BIT) == 0 {
            ctrl |= EMMC_CTRL_HISPD;
        } else {
            ctrl &= !EMMC_CTRL_HISPD;
        }

        debug!("EMMC Host Control 1: {:#x}", ctrl);

        self.write_reg8(EMMC_HOST_CTRL1, ctrl);

        if timing != MMC_TIMING_LEGACY && timing != MMC_TIMING_MMC_HS && timing != MMC_TIMING_SD_HS
        {
            self.sdhci_set_power(MMC_VDD_165_195_SHIFT).unwrap();
        }

        self.sdhci_set_uhs_signaling();
    }

    fn sdhci_get_version(&self) -> u16 {
        self.read_reg16(EMMC_HOST_CNTRL_VER) & 0xFF
    }
}
