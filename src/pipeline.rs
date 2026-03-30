//! 业务流水线（按 b→f→d→c→e 顺序）

use rust_decimal::Decimal;
use tokio::time::{interval, Duration};

use crate::components::SystemComponents;
use crate::tick_context::{
    BDataResult, CDataResult, DCheckResult, ERiskResult, FEngineResult,
    StageError, TickContext, SYMBOL, INITIAL_BALANCE,
};
use crate::utils::parse_raw_kline;

pub async fn run_pipeline(components: SystemComponents) -> Result<(), Box<dyn std::error::Error>> {
    let mut heartbeat_tick = interval(Duration::from_millis(1000));
    let mut loop_count = 0u64;
    let mut tick_count = 0u64;

    tracing::info!("Pipeline started");

    loop {
        tokio::select! {
            _ = heartbeat_tick.tick() => {
                tracing::trace!("[HB] alive #{}", loop_count);
            }

            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                loop_count += 1;

                let kline_data = {
                    let mut stream = components.kline_stream.lock().await;
                    stream.next_message()
                };

                let Some(data) = kline_data else {
                    tracing::info!("Data exhausted at loop {}", loop_count);
                    break;
                };

                let kline = match parse_raw_kline(&data) {
                    Ok(k) => k,
                    Err(e) => {
                        tracing::warn!("[b] Parse error: {}", e);
                        continue;
                    }
                };

                let mut ctx = TickContext::new(tick_count + 1, kline);

                stage_b_data(&mut ctx, tick_count + 1);
                stage_f_engine(&components, &mut ctx);
                let d_result = stage_d_check(&components, &mut ctx).await;
                ctx.visited.push("c");
                stage_e_risk(&components, &mut ctx, loop_count, &d_result);

                tick_count += 1;

                if ctx.errors.is_empty() {
                    tracing::debug!(
                        "[Tick#{}] b→f→d→c→e complete={} decision={}",
                        ctx.tick_id,
                        ctx.is_complete(),
                        ctx.d_check.as_ref().map(|d| d.decision.as_str()).unwrap_or("-")
                    );
                } else {
                    tracing::warn!("[Tick#{}] errors={:?}", ctx.tick_id, ctx.errors);
                }

                if tick_count % 100 == 0 {
                    tracing::info!(
                        "[Progress#{}] ticks={} {}",
                        loop_count,
                        tick_count,
                        serde_json::to_string(&ctx.to_report()).unwrap_or_default()
                    );
                }

                if loop_count >= 1000 {
                    tracing::info!("Max iterations reached");
                    break;
                }
            }
        }
    }

    tracing::info!("Pipeline done: {} loops, {} ticks", loop_count, tick_count);
    Ok(())
}

fn stage_b_data(ctx: &mut TickContext, kline_id: u64) {
    let valid = ctx.kline.close > Decimal::ZERO;
    ctx.b_data = Some(BDataResult {
        kline_id,
        valid,
    });
    ctx.visited.push("b");

    if !valid {
        ctx.errors.push(StageError {
            stage: "b".into(),
            code: "INVALID_PRICE".into(),
            detail: format!("close={} <= 0", ctx.kline.close),
        });
    }
}

fn stage_f_engine(components: &SystemComponents, ctx: &mut TickContext) {
    components.gateway.update_price(SYMBOL, ctx.kline.close);
    ctx.f_engine = Some(FEngineResult {
        price_updated: true,
        account_synced: true,
    });
    ctx.visited.push("f");
}

async fn stage_d_check(components: &SystemComponents, ctx: &mut TickContext) -> DCheckResult {
    let c_result = {
        let r = components.signal_processor.min_update(
            SYMBOL,
            ctx.kline.high,
            ctx.kline.low,
            ctx.kline.close,
            ctx.kline.volume,
        );

        CDataResult {
            zscore_14: None,
            tr_base: None,
            pos_norm: None,
            signal: r.is_ok(),
        }
    };
    ctx.c_data = Some(c_result);
    ctx.visited.push("c");

    let trade_result = components.trader.execute_once_wal().await;

    match &trade_result {
        Ok(d_checktable::h_15m::ExecutionResult::Executed { qty, .. }) => {
            let result = DCheckResult {
                decision: "long_entry".into(),
                qty: Some(*qty),
                reason: "signal_triggered".into(),
            };
            ctx.d_check = Some(result.clone());
            ctx.visited.push("d");
            result
        }
        Ok(d_checktable::h_15m::ExecutionResult::Skipped(reason)) => {
            let result = DCheckResult {
                decision: "skip".into(),
                qty: None,
                reason: reason.to_string(),
            };
            ctx.d_check = Some(result.clone());
            ctx.visited.push("d");
            result
        }
        Ok(d_checktable::h_15m::ExecutionResult::Failed(e)) => {
            ctx.errors.push(StageError {
                stage: "d".into(),
                code: "TRADE_FAILED".into(),
                detail: e.to_string(),
            });
            let result = DCheckResult {
                decision: "error".into(),
                qty: None,
                reason: e.to_string(),
            };
            ctx.d_check = Some(result.clone());
            ctx.visited.push("d");
            result
        }
        Err(e) => {
            ctx.errors.push(StageError {
                stage: "d".into(),
                code: "TRADE_ERROR".into(),
                detail: e.to_string(),
            });
            let result = DCheckResult {
                decision: "error".into(),
                qty: None,
                reason: e.to_string(),
            };
            ctx.d_check = Some(result.clone());
            ctx.visited.push("d");
            result
        }
    }
}

fn stage_e_risk(
    components: &SystemComponents,
    ctx: &mut TickContext,
    loop_id: u64,
    d_result: &DCheckResult,
) {
    let Some(qty) = d_result.qty else {
        ctx.visited.push("e");
        return;
    };

    let balance_passed = components
        .risk_checker
        .pre_check(
            SYMBOL,
            INITIAL_BALANCE,
            Decimal::try_from(100).unwrap(),
            INITIAL_BALANCE,
        )
        .is_ok();

    let order_check_result = components.order_checker.pre_check(
        &format!("order_{}", loop_id),
        SYMBOL,
        "h_15m_strategy",
        Decimal::try_from(100).unwrap(),
        INITIAL_BALANCE,
        Decimal::try_from(0).unwrap(),
    );
    let order_passed = order_check_result.passed;

    ctx.e_risk = Some(ERiskResult {
        balance_passed,
        order_passed,
    });
    ctx.visited.push("e");

    if balance_passed && order_passed {
        if let Ok(order) = components.gateway.place_order(SYMBOL, b_data_mock::api::mock_account::Side::Buy, qty, None) {
            tracing::info!(
                "[Tick#{}] [e] Filled: price={} qty={}",
                ctx.tick_id,
                order.filled_price,
                order.filled_qty
            );
        }
    } else {
        ctx.errors.push(StageError {
            stage: "e".into(),
            code: "RISK_REJECTED".into(),
            detail: format!("balance={} order={}", balance_passed, order_passed),
        });
    }
}
