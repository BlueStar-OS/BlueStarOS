use core::cell::UnsafeCell;

use alloc::{boxed::Box, vec::Vec};
use log::warn;

use crate::{
    config::PAGE_SIZE,
    driver::gpu::{get_font_bitmap, put_pixel},
    memory::FramTracker,
};

// ============================================================================
// ANSI SGR 颜色常量
// 参考：
// - Linux 5.4.29 `drivers/tty/vt/vt.c:184-235` (default_red/default_grn/default_blu)
// - ECMA-48 SGR 标准 (Select Graphic Rendition)
// ============================================================================

/// ANSI 16 色调色板 (ARGB 8:8:8:8 格式)。
///
/// 索引 0-7 为标准色，8-15 为高亮色。
/// Linux 对应 `drivers/tty/vt/vt.c:184-235`
const ANSI_PALETTE: [u32; 16] = [
    0x0000_0000, //  0: Black
    0x00AA_0000, //  1: Red
    0x0000_AA00, //  2: Green
    0x00AA_5500, //  3: Yellow / Brown
    0x0000_00AA, //  4: Blue
    0x00AA_00AA, //  5: Magenta
    0x0000_AAAA, //  6: Cyan
    0x00AA_AAAA, //  7: Light Gray
    0x0055_5555, //  8: Dark Gray
    0x00FF_5555, //  9: Light Red
    0x0055_FF55, // 10: Light Green
    0x00FF_FF55, // 11: Bright Yellow
    0x0055_55FF, // 12: Light Blue
    0x00FF_55FF, // 13: Light Magenta
    0x0055_FFFF, // 14: Light Cyan
    0x00FF_FFFF, // 15: White
];

// ============================================================================
// ANSI CSI 解析状态机
// ============================================================================

/// ANSI 转义序列解析状态。
///
/// 状态转换图：
/// ```text
/// Normal ──(ESC)──► Esc ──('[')──► CsiParsing ──(final byte)── apply ──► Normal
///                    │                         │
///                    └ other → print fallback   ├ digit → accumulate
///                                              ├ ';'   → push param
///                                              ├ '?'   → DEC private prefix
///                                              └ other private byte → ignore CSI
/// ```
#[derive(PartialEq, Debug)]
pub enum AnsiState {
    /// 正常打印字符模式
    Normal,
    /// 刚收到 ESC (0x1B)，等待 '['
    Esc,
    /// 正在解析 CSI 参数序列 `ESC [ params m`
    CsiParsing {
        /// 已收集的参数值列表，最多 8 个。
        params: [u32; 8],
        /// 当前已收集的参数个数
        param_count: usize,
        /// 当前正在构建的数
        current: u32,
        /// 是否收到过数字（区分 `\033[m` 即空参数=0 和 `\033[0m`）
        has_digit: bool,
        /// 是否正在跳过 DEC 私有序列，例如 `ESC[?25l`。
        private: bool,
    },
}

// ============================================================================
// Console 结构体
// ============================================================================

/// 图形化 VGA 终端。
///
/// 在 framebuffer 上用 8×16 点阵字体渲染文本，支持 ANSI SGR 颜色序列。
#[derive(Debug)]
pub struct Console {
    pub fb_base: *mut u32,             // 显存基址 (BAR0)
    pub soft_buffer: Vec<FramTracker>, // 软件帧缓冲
    pub width: usize,                  // 屏幕像素宽度 (如 512)
    pub height: usize,                 // 屏幕像素高度 (如 512)
    pub font_x: u8,                    // 字符像素宽度 (8)
    pub font_y: u8,                    // 字符像素高度 (16)
    pub cursor_x: usize,               // 当前光标 X (字符坐标)
    pub cursor_y: usize,               // 当前光标 Y (字符坐标)
    pub foreground: u32,               // 文字颜色 ARGB
    pub background: u32,               // 背景颜色 ARGB
    pub bold: bool,                    // SGR bold/intensity 标记
    pub state: AnsiState,              // ANSI 解析状态 (不公开)
}

pub static mut VgaScreen: Console = Console {
    fb_base: core::ptr::null_mut(),
    soft_buffer: Vec::new(),
    width: 0,
    height: 0,
    font_x: 8,
    font_y: 16,
    cursor_x: 0,
    cursor_y: 0,
    foreground: 0x00FF_FFFF,
    background: 0x0000_0000,
    bold: false,
    state: AnsiState::Normal,
};

impl Console {
    /// 向终端输出一个字符，自动识别 ANSI 转义序列并改变颜色。
    ///
    /// 支持的 ANSI SGR 命令：
    /// - `\033[0m` — 重置颜色
    /// - `\033[1m` — 高亮（切换为亮色）
    /// - `\033[30m`–`\033[37m` — 前景色
    /// - `\033[40m`–`\033[47m` — 背景色
    /// - `\033[90m`–`\033[97m` — 亮前景色
    /// - `\033[100m`–`\033[107m` — 亮背景色
    /// - 支持 `;` 连接多参数，如 `\033[1;32m`
    /// - 支持常见 `J/K/H/f` 控制序列，避免 shell/日志输出把控制码画到屏幕上。
    pub fn draw_char(&mut self, c: char) {
        // ---- ANSI escape/control sequence parsing ----
        //
        // Linux 5.4.29 参考：
        // - `drivers/tty/vt/vt.c:2107-2159`：控制字符可在 escape sequence 中生效；
        // - `drivers/tty/vt/vt.c:2249-2287`：CSI 参数收集；
        // - `drivers/tty/vt/vt.c:2378-2411`：按 final byte 分发 `J/K/m`。
        if self.handle_control_char(c) {
            return;
        }

        // 用 replace 取出 state 所有权，避免嵌套借用冲突
        // （CSI final 处理会调用 &mut self 方法，不能同时持有 &mut self.state）
        let state = core::mem::replace(&mut self.state, AnsiState::Normal);

        match state {
            AnsiState::Normal => {
                self.state = AnsiState::Normal;
            }

            AnsiState::Esc => {
                if c == '[' {
                    self.state = AnsiState::CsiParsing {
                        params: [0; 8],
                        param_count: 0,
                        current: 0,
                        has_digit: false,
                        private: false,
                    };
                } else {
                    // Linux 对未知 ESC 序列会回到 Normal。这里选择把后一个字符按普通
                    // 字符渲染，避免 `ESC` 被误打入日志后吞掉下一字节。
                    warn!("unsupported ansi escape: ESC followed by {:?}", c);
                    self.state = AnsiState::Normal;
                    self.render_printable_char(c);
                }
                return;
            }

            AnsiState::CsiParsing {
                mut params,
                mut param_count,
                mut current,
                has_digit: mut had_digit,
                mut private,
            } => match c {
                '0'..='9' => {
                    current = current.saturating_mul(10) + c.to_digit(10).unwrap();
                    had_digit = true;
                    self.state = AnsiState::CsiParsing {
                        params,
                        param_count,
                        current,
                        has_digit: had_digit,
                        private,
                    };
                    return;
                }
                ';' => {
                    Self::push_csi_param(&mut params, &mut param_count, current);
                    self.state = AnsiState::CsiParsing {
                        params,
                        param_count,
                        current: 0,
                        has_digit: false,
                        private,
                    };
                    return;
                }
                '?' => {
                    // DEC private prefix，例如 `ESC[?25l`。当前 VGA console 不维护光标
                    // 可见性等模式，但要完整吞掉它，不能把后续数字/尾字节画出来。
                    private = true;
                    self.state = AnsiState::CsiParsing {
                        params,
                        param_count,
                        current,
                        has_digit: had_digit,
                        private,
                    };
                    return;
                }
                '\x20'..='\x2f' | ':' | '<' | '=' | '>' => {
                    // CSI intermediate/private bytes。第一版不实现，吞掉整个 CSI。
                    self.state = AnsiState::Normal;
                    return;
                }
                '\x40'..='\x7e' => {
                    // 提交最后一个参数
                    if had_digit || param_count == 0 {
                        Self::push_csi_param(&mut params, &mut param_count, current);
                    }
                    self.apply_csi(c, &params[..param_count], private);
                    self.state = AnsiState::Normal;
                    return;
                }
                _ => {
                    self.state = AnsiState::Normal;
                    return;
                }
            },
        }

        self.render_printable_char(c);
    }

    /// 处理 ASCII 控制字符。
    ///
    /// 参考 Linux 5.4.29:
    /// - `drivers/tty/vt/vt.c:2107-2159`：控制字符可打断当前 escape 状态。
    fn handle_control_char(&mut self, c: char) -> bool {
        match c {
            '\x00' => true,
            '\x08' => {
                self.backspace();
                true
            }
            '\t' => {
                self.tab();
                true
            }
            '\n' | '\x0b' | '\x0c' => {
                self.new_line();
                true
            }
            '\r' => {
                self.cursor_x = 0;
                true
            }
            '\x18' | '\x1a' => {
                self.state = AnsiState::Normal;
                true
            }
            '\x1b' => {
                self.state = AnsiState::Esc;
                true
            }
            '\x7f' => true,
            _ => false,
        }
    }

    /// 压入一个 CSI 参数，超过上限时静默截断。
    fn push_csi_param(params: &mut [u32; 8], param_count: &mut usize, value: u32) {
        if *param_count < params.len() {
            params[*param_count] = value;
            *param_count += 1;
        }
    }

    /// 渲染一个普通可见字符。
    fn render_printable_char(&mut self, c: char) {
        if self.fb_base.is_null() {
            return;
        }

        let font_data = get_font_bitmap(c);

        let pixel_x = self.cursor_x * self.font_x as usize;
        let pixel_y = self.cursor_y * self.font_y as usize;

        for row in 0..self.font_y as usize {
            let row_data = font_data[row];
            for col in 0..self.font_x as usize {
                let color = if (row_data >> (7 - col)) & 1 == 1 {
                    self.foreground
                } else {
                    self.background
                };
                unsafe {
                    put_pixel(
                        self.fb_base as usize,
                        self.width,
                        pixel_x + col,
                        pixel_y + row,
                        color,
                    );
                }
            }
        }

        self.cursor_x += 1;
        let cols = self.width / self.font_x as usize;
        if self.cursor_x >= cols {
            self.new_line();
        }
    }

    /// 处理 CSI final byte。
    ///
    /// 参考 Linux 5.4.29:
    /// - `drivers/tty/vt/vt.c:2378-2411`：`J/K/m` 等 CSI 分发。
    fn apply_csi(&mut self, final_byte: char, params: &[u32], private: bool) {
        if private {
            // 暂不实现 DEC private modes，例如 `?25l/?25h` 光标显示隐藏。
            return;
        }

        match final_byte {
            'm' => self.apply_sgr(params),
            'J' => self.erase_display(params.first().copied().unwrap_or(0)),
            'K' => self.erase_line(params.first().copied().unwrap_or(0)),
            'H' | 'f' => {
                let row = params.first().copied().unwrap_or(1);
                let col = params.get(1).copied().unwrap_or(1);
                self.move_cursor_1_based(row, col);
            }
            'A' => self.move_cursor_relative(0, -(params.first().copied().unwrap_or(1) as isize)),
            'B' => self.move_cursor_relative(0, params.first().copied().unwrap_or(1) as isize),
            'C' => self.move_cursor_relative(params.first().copied().unwrap_or(1) as isize, 0),
            'D' => self.move_cursor_relative(-(params.first().copied().unwrap_or(1) as isize), 0),
            _ => {}
        }
    }

    // ---- ANSI SGR 命令处理 ----

    /// 应用 SGR (Select Graphic Rendition) 参数。
    ///
    /// 参考 ECMA-48 §8.3.117，Linux `drivers/tty/vt/vt.c:1686-1735`
    fn apply_sgr(&mut self, params: &[u32]) {
        let params = if params.is_empty() { &[0][..] } else { params };
        let mut i = 0;
        while i < params.len() {
            let code = params[i];
            match code {
                0 => {
                    // 重置所有属性
                    self.foreground = 0x00FF_FFFF; // white
                    self.background = 0x0000_0000; // black
                    self.bold = false;
                }
                1 => {
                    self.bold = true;
                }
                22 => {
                    self.bold = false;
                }
                39 => {
                    self.foreground = 0x00FF_FFFF;
                }
                49 => {
                    self.background = 0x0000_0000;
                }
                30..=37 => {
                    // 标准前景色
                    let bright_offset = if self.bold { 8 } else { 0 };
                    self.set_fg((code - 30) as usize + bright_offset);
                }
                40..=47 => {
                    // 标准背景色
                    self.set_bg((code - 40) as usize);
                }
                90..=97 => {
                    // 亮前景色
                    self.set_fg((code - 90) as usize + 8);
                }
                100..=107 => {
                    // 亮背景色
                    self.set_bg((code - 100) as usize + 8);
                }
                _ => {
                    // 未知 SGR 码，忽略
                }
            }
            i += 1;
        }
    }

    /// 设置前景色为调色板第 `idx` 号颜色。
    fn set_fg(&mut self, idx: usize) {
        if idx < ANSI_PALETTE.len() {
            self.foreground = ANSI_PALETTE[idx];
        }
    }

    /// 设置背景色为调色板第 `idx` 号颜色。
    fn set_bg(&mut self, idx: usize) {
        if idx < ANSI_PALETTE.len() {
            self.background = ANSI_PALETTE[idx];
        }
    }

    // ---- 光标控制 ----

    /// 换行：cursor_x 归零，cursor_y 加一；超出底部则触发 scroll_up。
    fn new_line(&mut self) {
        self.cursor_x = 0;
        self.cursor_y += 1;
        let rows = self.height / self.font_y as usize;
        if self.cursor_y >= rows {
            self.scroll_up();
        }
    }

    /// 退格：只移动光标，不擦除字符。
    fn backspace(&mut self) {
        self.cursor_x = self.cursor_x.saturating_sub(1);
    }

    /// Tab：移动到下一个 8 字符边界。
    fn tab(&mut self) {
        let cols = self.cols();
        let next_tab = (self.cursor_x + 8) & !7;
        self.cursor_x = core::cmp::min(next_tab, cols.saturating_sub(1));
    }

    /// 字符列数。
    fn cols(&self) -> usize {
        core::cmp::max(1, self.width / self.font_x as usize)
    }

    /// 字符行数。
    fn rows(&self) -> usize {
        core::cmp::max(1, self.height / self.font_y as usize)
    }

    /// 移动到 ANSI 1-based 坐标。
    fn move_cursor_1_based(&mut self, row: u32, col: u32) {
        let target_y = row.saturating_sub(1) as usize;
        let target_x = col.saturating_sub(1) as usize;
        self.cursor_x = core::cmp::min(target_x, self.cols().saturating_sub(1));
        self.cursor_y = core::cmp::min(target_y, self.rows().saturating_sub(1));
    }

    /// 相对移动光标。
    fn move_cursor_relative(&mut self, dx: isize, dy: isize) {
        let x = (self.cursor_x as isize).saturating_add(dx);
        let y = (self.cursor_y as isize).saturating_add(dy);
        self.cursor_x = x.clamp(0, self.cols().saturating_sub(1) as isize) as usize;
        self.cursor_y = y.clamp(0, self.rows().saturating_sub(1) as isize) as usize;
    }

    /// 清除屏幕区域。
    ///
    /// mode:
    /// - 0: 从光标到屏幕末尾；
    /// - 1: 从屏幕开头到光标；
    /// - 2/3: 整屏。
    ///
    /// 参考 Linux 5.4.29 `drivers/tty/vt/vt.c:1509-1543`。
    fn erase_display(&mut self, mode: u32) {
        match mode {
            0 => {
                self.erase_cells(self.cursor_x, self.cursor_y, self.cols() - self.cursor_x);
                for row in self.cursor_y + 1..self.rows() {
                    self.erase_cells(0, row, self.cols());
                }
            }
            1 => {
                for row in 0..self.cursor_y {
                    self.erase_cells(0, row, self.cols());
                }
                self.erase_cells(0, self.cursor_y, self.cursor_x + 1);
            }
            2 | 3 => {
                for row in 0..self.rows() {
                    self.erase_cells(0, row, self.cols());
                }
                if mode == 2 {
                    self.cursor_x = 0;
                    self.cursor_y = 0;
                }
            }
            _ => {}
        }
    }

    /// 清除当前行区域。
    ///
    /// 参考 Linux 5.4.29 `drivers/tty/vt/vt.c:1546-1565`。
    fn erase_line(&mut self, mode: u32) {
        match mode {
            0 => self.erase_cells(self.cursor_x, self.cursor_y, self.cols() - self.cursor_x),
            1 => self.erase_cells(0, self.cursor_y, self.cursor_x + 1),
            2 => self.erase_cells(0, self.cursor_y, self.cols()),
            _ => {}
        }
    }

    /// 用当前背景色擦除一段字符格。
    fn erase_cells(&mut self, start_col: usize, row: usize, count: usize) {
        let start_x = start_col * self.font_x as usize;
        let start_y = row * self.font_y as usize;
        let width = count * self.font_x as usize;
        let height = self.font_y as usize;
        self.fill_rect(start_x, start_y, width, height, self.background);
    }

    /// 填充一个像素矩形。
    fn fill_rect(&mut self, x: usize, y: usize, width: usize, height: usize, color: u32) {
        let max_y = core::cmp::min(y + height, self.height);
        let max_x = core::cmp::min(x + width, self.width);
        for py in y..max_y {
            for px in x..max_x {
                unsafe {
                    put_pixel(self.fb_base as usize, self.width, px, py, color);
                }
            }
        }
    }

    /// 屏幕上滚一字符行（font_y 像素高）。
    ///
    /// 流程：把第 1 行及之后的内容整体搬到第 0 行（memmove），然后清空最后一行。
    /// 参考 Linux 5.4.29 `drivers/video/fbdev/core/bitblit.c:378-410`
    fn scroll_up(&mut self) {
        let fb_width_px = self.width;
        let char_height_px = self.font_y as usize;
        // framebuffer 按 `u32` 像素访问，所以 copy/write_bytes 的 count 都必须是像素数。
        let char_row_pixels = fb_width_px * char_height_px;
        let fb_total_pixels = fb_width_px * self.height;

        unsafe {
            // 把第 1 字符行开始的内容搬到第 0 行
            let src = self.fb_base.add(fb_width_px * char_height_px);
            let dst = self.fb_base;
            core::ptr::copy(src, dst, fb_total_pixels - char_row_pixels);

            // 清空最后一行
            let last_line = self
                .fb_base
                .add(fb_width_px * (self.height - char_height_px));
            core::ptr::write_bytes(last_line, 0, char_row_pixels);
        }

        self.cursor_y = self.cursor_y.saturating_sub(1);
    }

    // ---- 测试函数 ----

    /// 连续输出 a-z 无限循环，用于测试滚动。
    ///
    /// 字符填满屏幕底部后自动触发 scroll_up，可直观观察滚动是否正确。
    pub fn scroll_test(&mut self) {
        let mut ch: u8 = b'a';
        loop {
            self.draw_char(ch as char);
            // 忙等减速，方便观察
            for _ in 0..100000 {
                core::hint::spin_loop();
            }
            ch += 1;
            if ch > b'z' {
                ch = b'a';
                self.draw_char('\n');
            }
        }
    }
}
