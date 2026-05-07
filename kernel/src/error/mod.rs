//! BlueStarOS 错误模块
//! 对齐linux的标准返回标准的错误码代替-1等硬编码

mod transform;
mod r#type;

pub use r#type::BlueErr;
