//! 工具模块
//!
//! 包含:
//! - TelegramNotifier: Telegram 通知器

pub mod telegram_notifier;

pub use telegram_notifier::{TelegramConfig, TelegramNotifier};
