//! 检查模块
//!
//! 流水线: a_exit → b_close → c_hedge → d_add → e_open
//! 每个检查独立执行，通过则发送对应信号，不通过则无事发生

pub mod a_exit;
pub mod b_close;
pub mod c_hedge;
pub mod d_add;
pub mod e_open;

pub mod check_chain;

// Re-export CheckChainContext
pub use check_chain::CheckChainContext;
