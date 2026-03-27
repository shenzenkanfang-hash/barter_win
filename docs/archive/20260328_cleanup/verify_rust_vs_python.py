#!/usr/bin/env python3
"""
验证 Rust pine_debug.rs 的计算结果与 Python pine_scripts.py 是否一致
使用方法: python verify_rust_vs_python.py
"""

import pandas as pd
import numpy as np
from io import StringIO

# 读取 Python 的 CSV 数据（包含原始 K 线和计算结果）
python_csv_path = "D:/量化策略开发/tradingW/1d_pine.csv"
python_df = pd.read_csv(python_csv_path)

print(f"Python CSV 数据行数: {len(python_df)}")
print(f"列名: {list(python_df.columns)}")
print()

# 读取 Rust 的输出 CSV
rust_csv_path = "D:/Rust项目/barter-rs-main/pine_btcusdt_1000.csv"
rust_df = pd.read_csv(rust_csv_path, header=None, names=['date', 'index', 'close', 'macd', 'signal', 'hist', 'ema10', 'ema20', 'rsi', 'bar_color', 'bg_color'])

print(f"Rust CSV 数据行数: {len(rust_df)}")
print(f"列名: {list(rust_df.columns)}")
print()

# 对比最后 20 行
print("=" * 80)
print("对比最后 20 行:")
print("=" * 80)

# Python 数据（包含 timestamp, open, high, low, close, volume, pine_bar_color_100_200, pine_bg_color_100_200）
python_last20 = python_df.tail(20).copy()
rust_last20 = rust_df.tail(20).copy()

# 打印对比
for i, (py_row, rust_row) in enumerate(zip(python_last20.itertuples(), rust_last20.itertuples())):
    py_date = getattr(py_row, 'timestamp', 'N/A')
    rust_date = rust_row.date

    py_close = getattr(py_row, 'close', 'N/A')
    rust_close = rust_row.close

    py_bar = getattr(py_row, 'pine_bar_color_100_200', 'N/A')
    rust_bar = rust_row.bar_color

    py_bg = getattr(py_row, 'pine_bg_color_100_200', 'N/A')
    rust_bg = rust_row.bg_color

    rust_ema10 = rust_row.ema10
    rust_ema20 = rust_row.ema20
    rust_rsi = rust_row.rsi

    match = "✓" if py_close == rust_close else "✗"

    print(f"{py_date} | close: {py_close} vs {rust_close} {match}")
    print(f"  Python: bar={py_bar}, bg={py_bg}")
    print(f"  Rust:   bar={rust_bar}, bg={rust_bg}")
    print(f"  Rust EMA10={rust_ema10:.2f}, EMA20={rust_ema20:.2f}, RSI={rust_rsi:.2f}")
    print()

print("=" * 80)
print("说明:")
print("1. 如果 close 价格不一致，说明数据源不同，需要使用相同的 K 线数据")
print("2. 如果 close 一致但颜色不一致，说明计算逻辑有差异")
print("=" * 80)