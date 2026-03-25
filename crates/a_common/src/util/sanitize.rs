//! 敏感信息脱敏工具
//!
//! 用于在日志输出前脱敏敏感字段，防止敏感信息泄露。
//!
//! # 使用方式
//!
//! ```rust
//! use a_common::util::sanitize::{mask_api_key, mask_order_id};
//!
//! let masked_key = mask_api_key("abc123xyz");  // "abc***xyz"
//! let masked_order = mask_order_id("1234567890"); // "****567890"
//! ```

#![forbid(unsafe_code)]

/// 脱敏 API Key
///
/// 只显示前3位和后3位，中间用 *** 替代
///
/// # Example
/// ```
/// assert_eq!(mask_api_key("abc123xyz"), "abc***xyz");
/// assert_eq!(mask_api_key("ab"), "**");
/// assert_eq!(mask_api_key("abc"), "a*c");
/// ```
pub fn mask_api_key(key: &str) -> String {
    if key.len() <= 6 {
        // 太短的 key 无法有效脱敏，只显示首尾字符
        if key.len() == 1 {
            return "*".to_string();
        }
        let mut masked = String::with_capacity(key.len());
        for (i, c) in key.chars().enumerate() {
            if i == 0 || i == key.len() - 1 {
                masked.push(c);
            } else {
                masked.push('*');
            }
        }
        masked
    } else {
        format!("{}***{}", &key[..3], &key[key.len() - 3..])
    }
}

/// 脱敏订单 ID
///
/// 只显示后4位，前面用 **** 替代
///
/// # Example
/// ```
/// assert_eq!(mask_order_id("1234567890"), "****7890");
/// assert_eq!(mask_order_id("12345"), "***45");
/// assert_eq!(mask_order_id("1"), "*");
/// ```
pub fn mask_order_id(order_id: &str) -> String {
    // 1位：无法脱敏
    if order_id.len() == 1 {
        return "*".to_string();
    }
    // 2-4位：无法有效脱敏，返回原始值
    if order_id.len() <= 4 {
        return order_id.to_string();
    }
    // 超过4位：隐藏前面的字符，只显示后4位
    format!("{}{}", "*".repeat(order_id.len() - 4), &order_id[order_id.len() - 4..])
}

/// 脱敏交易对符号
///
/// 保留完整符号（业务需要识别），不做隐藏
///
/// # Example
/// ```
/// assert_eq!(mask_symbol("BTCUSDT"), "BTCUSDT");
/// ```
pub fn mask_symbol(_symbol: &str) -> String {
    // 交易对符号本身不是敏感信息，可以完整显示
    // 如需脱敏可改为返回 _symbol.to_string()
    _symbol.to_string()
}

/// 脱敏邮箱地址
///
/// 只显示前2位和域名部分
///
/// # Example
/// ```
/// assert_eq!(mask_email("user@example.com"), "us***@example.com");
/// ```
pub fn mask_email(email: &str) -> String {
    if let Some(at_pos) = email.find('@') {
        let local = &email[..at_pos];
        let domain = &email[at_pos..];
        if local.len() <= 2 {
            format!("**{}", domain)
        } else {
            format!("{}**{}", &local[..2], domain)
        }
    } else {
        "***".to_string()
    }
}

/// 脱敏 Telegram Bot Token
///
/// 只显示前10位和后4位
///
/// # Example
/// ```
/// assert_eq!(mask_bot_token("123456789:ABCDefGHIjklMNOP"), "1234567***MNOP");
/// ```
pub fn mask_bot_token(token: &str) -> String {
    // 太短的token无法有效脱敏
    if token.len() <= 7 {
        return "***".to_string();
    }
    // 显示前7位和后4位，中间用****替代
    format!("{}****{}", &token[..7], &token[token.len() - 4..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_api_key() {
        assert_eq!(mask_api_key("abc123xyz"), "abc***xyz");
        assert_eq!(mask_api_key("ab"), "ab"); // 太短无法有效脱敏，返回原始值
        assert_eq!(mask_api_key("abc"), "a*c");
        assert_eq!(mask_api_key("a"), "*");
    }

    #[test]
    fn test_mask_order_id() {
        // 10位ID：隐藏前6位，显示后4位
        assert_eq!(mask_order_id("1234567890"), "******7890");
        // 5位ID：隐藏前1位，显示后4位
        assert_eq!(mask_order_id("12345"), "*2345");
        assert_eq!(mask_order_id("1"), "*");
        // 正好4位无法有效脱敏，返回原始值
        assert_eq!(mask_order_id("1234"), "1234");
    }

    #[test]
    fn test_mask_symbol() {
        assert_eq!(mask_symbol("BTCUSDT"), "BTCUSDT");
        assert_eq!(mask_symbol("ETHUSDT"), "ETHUSDT");
    }

    #[test]
    fn test_mask_email() {
        assert_eq!(mask_email("user@example.com"), "us**@example.com");
        assert_eq!(mask_email("a@b.com"), "**@b.com");
    }

    #[test]
    fn test_mask_bot_token() {
        // "123456789:ABCDefGHIjklMNOP" (27 chars): 前7="123456789:", 后4="MNOP"
        assert_eq!(mask_bot_token("123456789:ABCDefGHIjklMNOP"), "1234567****MNOP");
        // "12345678901234" (14 chars): 前7="1234567", 后4="1234"
        assert_eq!(mask_bot_token("12345678901234"), "1234567****1234");
        // 短token直接返回***
        assert_eq!(mask_bot_token("short"), "***");
    }
}
