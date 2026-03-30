技术栈
===

编程语言
---
Rust（2024 版本）

异步运行时
---
tokio 版本 1，功能特性 ["full"]

金融精度
---
rust_decimal 版本 1.36，功能特性 ["maths"]

并发原语
---
parking_lot 版本 0.12

错误处理
---
thiserror 版本 2.0

序列化
---
serde 版本 1.0，含 derive 功能

时间处理
---
chrono 版本 0.4，含 serde 功能

数据库
---
rusqlite 版本 0.32，功能特性 ["bundled"]

工作区配置
---
Resolver 版本 "3"

功能标志
---
mock 功能用于沙箱模式

平台路径
---
Windows：E:/shm/backup/
Linux：/dev/shm/backup/

Cargo 工作区成员
---
a_common
b_data_source
b_data_mock
c_data_process
d_checktable
e_risk_monitor
f_engine
x_data
g_test
