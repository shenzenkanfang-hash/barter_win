#![forbid(unsafe_code)]

use crate::types::{PriceControlInput, PriceControlOutput};
use crate::min::price_control_generator::MinPriceControlGenerator;

/// 日线级价格控制器 (复用分钟级实现)
pub type DayPriceControlGenerator = MinPriceControlGenerator;
