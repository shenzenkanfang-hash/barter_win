# 量化交易系统 - Rust

六层架构: a_common → b_data_source → c_data_process → d_checktable → e_risk_monitor → f_engine
                               ↓
                          h_sandbox

入口: src/main.rs / src/sandbox_pure.rs
沙盒: crates/h_sandbox/src/
执行器: crates/d_checktable/src/

规则:
1. 只动代码，不动文档
2. 沙盒只注入原始数据，不计算指标
3. 共享Store用Arc，不用两份实例
