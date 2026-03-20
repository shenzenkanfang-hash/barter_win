//! Telegram 通知器 - 发送交易事件通知
//!
//! 支持的通知类型：
//! - 订单成交
//! - 订单被拒绝
//! - 通道切换
//! - 强制平仓
//! - 账户快照

use crate::error::EngineError;
use reqwest::Client;
use serde::Serialize;

/// Telegram 通知器配置
#[derive(Debug, Clone)]
pub struct TelegramConfig {
    /// Bot Token
    pub bot_token: String,
    /// Chat ID
    pub chat_id: String,
}

/// Telegram 消息请求
#[derive(Serialize)]
struct SendMessageRequest {
    chat_id: String,
    text: String,
    parse_mode: Option<String>,
}

/// Telegram 通知器
#[derive(Debug, Clone)]
pub struct TelegramNotifier {
    bot_token: String,
    chat_id: String,
    client: Client,
}

impl TelegramNotifier {
    /// 创建 Telegram 通知器
    pub fn new(bot_token: String, chat_id: String) -> Self {
        Self {
            bot_token,
            chat_id,
            client: Client::new(),
        }
    }

    /// 从配置创建 Telegram 通知器
    pub fn from_config(config: TelegramConfig) -> Self {
        Self::new(config.bot_token, config.chat_id)
    }

    /// 发送消息
    pub async fn send(&self, message: &str) -> Result<(), EngineError> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );

        let request = SendMessageRequest {
            chat_id: self.chat_id.clone(),
            text: message.to_string(),
            parse_mode: Some("Markdown".to_string()),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| EngineError::Network(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(EngineError::Network(format!(
                "Telegram API error: {}",
                response.status()
            )))
        }
    }

    // =========================================================================
    // 便捷通知方法
    // =========================================================================

    /// 通知订单成交
    pub async fn notify_order_filled(
        &self,
        symbol: &str,
        side: &str,
        price: &str,
        qty: &str,
    ) -> Result<(), EngineError> {
        let emoji = if side == "LONG" { "🟢" } else { "🔴" };
        let msg = format!(
            "{} *订单成交*\n\
            品种: `{symbol}`\n\
            方向: {side}\n\
            价格: `{price}`\n\
            数量: {qty}",
            emoji,
            symbol = symbol,
            side = side,
            price = price,
            qty = qty
        );
        self.send(&msg).await
    }

    /// 通知订单被拒绝
    pub async fn notify_order_rejected(
        &self,
        symbol: &str,
        reason: &str,
    ) -> Result<(), EngineError> {
        let msg = format!(
            "🔴 *订单被拒绝*\n\
            品种: `{symbol}`\n\
            原因: {reason}",
            symbol = symbol,
            reason = reason
        );
        self.send(&msg).await
    }

    /// 通知通道切换
    pub async fn notify_channel_switch(
        &self,
        from: &str,
        to: &str,
        volatility: &str,
    ) -> Result<(), EngineError> {
        let msg = format!(
            "⚡ *通道切换*\n\
            {from} → {to}\n\
            波动率: {volatility}",
            from = from,
            to = to,
            volatility = volatility
        );
        self.send(&msg).await
    }

    /// 通知强制平仓
    pub async fn notify_liquidation(
        &self,
        symbol: &str,
        reason: &str,
    ) -> Result<(), EngineError> {
        let msg = format!(
            "🚨 *强制平仓*\n\
            品种: `{symbol}`\n\
            原因: {reason}",
            symbol = symbol,
            reason = reason
        );
        self.send(&msg).await
    }

    /// 通知账户快照
    pub async fn notify_account_snapshot(
        &self,
        equity: &str,
        positions: &str,
    ) -> Result<(), EngineError> {
        let msg = format!(
            "📊 *账户快照*\n\
            权益: `{equity}`\n\
            持仓: {positions}",
            equity = equity,
            positions = positions
        );
        self.send(&msg).await
    }

    /// 通知高波动模式进入
    pub async fn notify_enter_high_volatility(
        &self,
        volatility: &str,
        tr_ratio: &str,
    ) -> Result<(), EngineError> {
        let msg = format!(
            "🔺 *进入高波动模式*\n\
            波动率: {volatility}\n\
            TR比率: {tr_ratio}",
            volatility = volatility,
            tr_ratio = tr_ratio
        );
        self.send(&msg).await
    }

    /// 通知高波动模式退出
    pub async fn notify_exit_high_volatility(
        &self,
        reason: &str,
    ) -> Result<(), EngineError> {
        let msg = format!(
            "🔻 *退出高波动模式*\n\
            原因: {reason}",
            reason = reason
        );
        self.send(&msg).await
    }

    /// 通知风控拒绝
    pub async fn notify_risk_rejected(
        &self,
        symbol: &str,
        reason: &str,
    ) -> Result<(), EngineError> {
        let msg = format!(
            "🛡️ *风控拒绝*\n\
            品种: `{symbol}`\n\
            原因: {reason}",
            symbol = symbol,
            reason = reason
        );
        self.send(&msg).await
    }
}

impl Default for TelegramNotifier {
    /// 创建一个禁用的通知器（不发送任何消息）
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            chat_id: String::new(),
            client: Client::new(),
        }
    }
}

impl TelegramNotifier {
    /// 检查通知器是否已配置
    pub fn is_configured(&self) -> bool {
        !self.bot_token.is_empty() && !self.chat_id.is_empty()
    }
}

// ============================================================================
// TelegramNotifier 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telegram_config_creation() {
        let config = TelegramConfig {
            bot_token: "test_token".to_string(),
            chat_id: "test_chat".to_string(),
        };

        let notifier = TelegramNotifier::from_config(config);
        assert!(notifier.is_configured());
    }

    #[test]
    fn test_default_notifier_not_configured() {
        let notifier = TelegramNotifier::default();
        assert!(!notifier.is_configured());
    }
}
