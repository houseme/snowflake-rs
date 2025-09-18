# Snowflake-Me

[English](README.md) | 简体中文

[![Crates.io](https://img.shields.io/crates/v/snowflake_me.svg)](https://crates.io/crates/snowflake_me)
[![Docs.rs](https://docs.rs/snowflake_me/badge.svg)](https://docs.rs/snowflake_me)
[![Build Status](https://github.com/houseme/snowflake-rs/workflows/Build/badge.svg)](https://github.com/houseme/snowflake-rs/actions?query=workflow%3ABuild)
[![License](https://img.shields.io/crates/l/snowflake-me)](LICENSE-APACHE)

一个高性能、高并发、分布式的 Rust Snowflake ID 生成器实现。

此版本的实现是 **无锁（Lock-Free）** 的，专为在多核 CPU 上实现最大吞吐量和最低延迟而设计。

## 设计亮点

- **无锁并发**：使用 `AtomicU64` 和 CAS (Compare-And-Swap) 操作来管理内部状态，完全消除了 `Mutex` 锁带来的线程争用和上下文切换开销。
- **高性能**：由于无锁设计，ID 生成速度极快，在高并发场景下表现出色。
- **高度可定制**：通过 `Builder` 模式，您可以灵活配置以下参数：
    - `start_time`: 起始时间戳，用于缩短生成 ID 的时间部分。
    - `machine_id` 和 `data_center_id`: 机器和数据中心标识符。
    - 各部分位长 (`time`, `sequence`, `machine_id`, `data_center_id`)。
- **智能 IP 地址兜底**：启用 `ip-fallback` 特性后，如果未提供 `machine_id` 或 `data_center_id`，系统会自动从本机网络接口获取。
    - **同时支持 IPv4 和 IPv6**：优先使用私有 IPv4 地址，若无则回退到私有 IPv6 地址。
    - **避免冲突**：为确保唯一性，`machine_id` 和 `data_center_id` 从 IP 地址的**不同部分**派生：
        - **IPv4**: `data_center_id` 来自第 3 字节，`machine_id` 来自第 4 字节。
        - **IPv6**: `data_center_id` 来自倒数第 2 个段，`machine_id` 来自最后 1 个段。
- **线程安全**：`Snowflake` 实例可以被安全地克隆（`clone`）并在多个线程之间共享，克隆操作非常轻量（仅增加 `Arc` 的引用计数）。

## Snowflake ID 结构

生成的 ID 是一个 64 位整数（`u64`），其结构如下（默认配置）：

```text
+-------------------------------------------------------------------------------------------------+
| 1 Bit (未用，符号位) | 41 Bits (时间戳，毫秒) | 5 Bits (数据中心 ID) | 5 Bits (机器 ID) | 12 Bits (序列号) |
+-------------------------------------------------------------------------------------------------+
```

- **符号位 (1 bit)**: 始终为 0，确保生成的 ID 为正数。
- **时间戳 (41 bits)**: 从您设定的 `start_time` 开始的毫秒数。41 位可以表示约 69 年的时间。
- **数据中心 ID (5 bits)**: 允许最多 32 个数据中心。
- **机器 ID (5 bits)**: 每个数据中心允许最多 32 台机器。
- **序列号 (12 bits)**: 表示在同一毫秒内，一台机器上可以生成的 ID 数量。12 位允许每毫秒生成 4096 个 ID。

**注意**：所有部分的位长都是可以通过 `Builder` 自定义的，但总和必须为 63 位。

## 快速开始

### 1. 添加依赖

将此库添加到您的 `Cargo.toml` 文件中：

```toml
[dependencies]
snowflake_me = "0.4.0" # 请使用最新版本
```

如果需要 IP 地址自动回退功能，请启用 `ip-fallback` 特性：

```toml
[dependencies]
snowflake_me = { version = "0.4.0", features = ["ip-fallback"] }
```

### 2. 基本用法

```rust
use snowflake_me::Snowflake;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 使用默认配置创建一个生成器
    // 注意：默认配置需要 `ip-fallback` 特性来自动获取机器 ID 和数据中心 ID
    let sf = Snowflake::new()?;

    // 生成一个唯一的 ID
    let id = sf.next_id()?;
    println!("Generated Snowflake ID: {}", id);

    Ok(())
}
```

### 3. 多线程用法

`Snowflake` 实例可以被高效地克隆并在线程间共享。

```rust
use snowflake_me::Snowflake;
use std::thread;
use std::sync::Arc;
use std::collections::HashSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 使用 Builder 手动配置 machine_id 和 data_center_id，这是生产环境中的推荐做法
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(10))
        .data_center_id(&|| Ok(5))
        .finalize()?;

    let sf_arc = Arc::new(sf);
    let mut handles = vec![];

    for _ in 0..10 {
        let sf_clone = Arc::clone(&sf_arc);
        let handle = thread::spawn(move || {
            let mut ids = Vec::new();
            for _ in 0..10000 {
                ids.push(sf_clone.next_id().unwrap());
            }
            ids
        });
        handles.push(handle);
    }

    let mut all_ids = HashSet::new();
    for handle in handles {
        let ids = handle.join().unwrap();
        for id in ids {
            // 验证所有 ID 是否唯一
            assert!(all_ids.insert(id), "Found duplicate ID: {}", id);
        }
    }

    println!("Successfully generated {} unique IDs across 10 threads.", all_ids.len());
    Ok(())
}
```

### 4. 分解 ID

您可以将一个 Snowflake ID 分解回其组成部分，以进行调试或分析。

```rust
use snowflake_me::{Snowflake, DecomposedSnowflake};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 确保使用与生成时相同的位长配置
    let bit_len_time = 41;
    let bit_len_sequence = 12;
    let bit_len_data_center_id = 5;
    let bit_len_machine_id = 5;

    let sf = Snowflake::builder()
        .bit_len_time(bit_len_time)
        .bit_len_sequence(bit_len_sequence)
        .bit_len_data_center_id(bit_len_data_center_id)
        .bit_len_machine_id(bit_len_machine_id)
        .machine_id(&|| Ok(15))
        .data_center_id(&|| Ok(7))
        .finalize()?;

    let id = sf.next_id()?;
    let decomposed = DecomposedSnowflake::decompose(
        id,
        bit_len_time,
        bit_len_sequence,
        bit_len_data_center_id,
        bit_len_machine_id,
    );

    println!("ID: {}", decomposed.id);
    println!("Time: {}", decomposed.time);
    println!("Data Center ID: {}", decomposed.data_center_id);
    println!("Machine ID: {}", decomposed.machine_id);
    println!("Sequence: {}", decomposed.sequence);

    assert_eq!(decomposed.machine_id, 15);
    assert_eq!(decomposed.data_center_id, 7);

    Ok(())
}
```

## 贡献

欢迎提交问题（Issues）和拉取请求（Pull Requests）。

## 许可证

本项目采用 [MIT](LICENSE-MIT) 和 [Apache 2.0](LICENSE-APACHE) 双重许可证。
