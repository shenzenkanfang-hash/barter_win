@echo off
REM b_data_source 测试脚本

set RUSTC=C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\rustc.exe
set CARGO=C:\Users\char\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe

cd /d "D:\Rust项目\barter-rs-main"

echo ========================================
echo 运行 b_data_source 所有测试
echo ========================================
%CARGO% test -p b_data_source --lib
if errorlevel 1 goto :fail

echo.
echo ========================================
echo 运行 futures 测试
echo ========================================
%CARGO% test -p b_data_source --lib tests::test_futures
if errorlevel 1 goto :fail

echo.
echo ========================================
echo 运行 kline 测试
echo ========================================
%CARGO% test -p b_data_source --lib tests::test_kline
if errorlevel 1 goto :fail

echo.
echo ========================================
echo 运行 orderbook 测试
echo ========================================
%CARGO% test -p b_data_source --lib tests::test_orderbook
if errorlevel 1 goto :fail

echo.
echo ========================================
echo 运行 models 测试
echo ========================================
%CARGO% test -p b_data_source --lib tests::test_models
if errorlevel 1 goto :fail

echo.
echo ========================================
echo 运行 symbol_registry 测试
echo ========================================
%CARGO% test -p b_data_source --lib tests::test_symbol_registry
if errorlevel 1 goto :fail

echo.
echo ========================================
echo 运行 recovery 测试
echo ========================================
%CARGO% test -p b_data_source --lib tests::test_recovery
if errorlevel 1 goto :fail

echo.
echo ========================================
echo 所有测试通过!
echo ========================================
goto :end

:fail
echo.
echo ========================================
echo 测试失败!
echo ========================================
exit /b 1

:end
