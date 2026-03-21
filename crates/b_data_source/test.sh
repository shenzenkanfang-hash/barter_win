#!/bin/bash
# b_data_source 测试脚本

set -e

export RUSTC="/c/Users/char/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin/rustc.exe"
export CARGO="/c/Users/char/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin/cargo.exe"

cd "D:\Rust项目\barter-rs-main"

echo "========================================"
echo "运行 b_data_source 所有测试"
echo "========================================"

$CARGO test -p b_data_source --lib

echo ""
echo "========================================"
echo "运行 futures 测试"
echo "========================================"
$CARGO test -p b_data_source --lib tests::test_futures

echo ""
echo "========================================"
echo "运行 kline 测试"
echo "========================================"
$CARGO test -p b_data_source --lib tests::test_kline

echo ""
echo "========================================"
echo "运行 orderbook 测试"
echo "========================================"
$CARGO test -p b_data_source --lib tests::test_orderbook

echo ""
echo "========================================"
echo "运行 models 测试"
echo "========================================"
$CARGO test -p b_data_source --lib tests::test_models

echo ""
echo "========================================"
echo "运行 symbol_registry 测试"
echo "========================================"
$CARGO test -p b_data_source --lib tests::test_symbol_registry

echo ""
echo "========================================"
echo "运行 recovery 测试"
echo "========================================"
$CARGO test -p b_data_source --lib tests::test_recovery

echo ""
echo "========================================"
echo "所有测试通过!"
echo "========================================"
