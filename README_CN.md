# Snowflake-Me

[English](README.md) | 简体中文

[![Crates.io](https://img.shields.io/crates/v/snowflake-me.svg)](https://crates.io/crates/snowflake-me)
[![Docs.rs](https://docs.rs/snowflake_me/badge.svg)](https://docs.rs/snowflake_me)
[![Build Status](https://github.com/houseme/snowflake-rs/workflows/Build/badge.svg)](https://github.com/houseme/snowflake-rs/actions?query=workflow%3ABuild)
[![License](https://img.shields.io/crates/l/snowflake-me)](LICENSE-APACHE)

一个高性能、高并发、分布式的 Rust Snowflake ID 生成器实现。

此版本的实现是 **无锁（Lock-Free）** 的，专为在多核 CPU 上实现最大吞吐量和最低延迟而设计。

## 设计亮点

- **无锁并发**：使用 `AtomicU64` 和 CAS (Compare-And-Swap) 操作来管理内部状态，完全消除了 `Mutex` 锁带来的线程争用和上下文切换开销。
- **高性能**：缓存行对齐（`#[repr(align(64))]`）防止线程间伪共享。CAS 循环默认使用 `compare_exchange_weak`，在 ARM 等架构上获得更好的吞吐量。
- **高度可定制**：通过 `Builder` 模式，您可以灵活配置以下参数：
    - `start_time`: 起始时间戳，用于缩短生成 ID 的时间部分。
    - `machine_id` 和 `data_center_id`: 机器和数据中心标识符。
    - 各部分位长 (`time`, `sequence`, `machine_id`, `data_center_id`)。
    - 时钟漂移策略和最大允许漂移量。
- **批量生成**：通过 `next_ids(count)` 单次调用生成多个唯一 ID，分摊调用开销。
- **智能 IP 地址兜底**：启用 `ip-fallback` 特性后，如果未提供 `machine_id` 或 `data_center_id`，系统会自动从本机网络接口获取。
    - **同时支持 IPv4 和 IPv6**：优先使用私有 IPv4 地址，若无则回退到私有 IPv6 地址。
    - **避免冲突**：为确保唯一性，`machine_id` 和 `data_center_id` 从 IP 地址的**不同部分**派生。
- **`no_std` 支持**：支持 `no_std` + `alloc` 环境，由用户提供时间源。
- **线程安全**：`Snowflake` 实例可以被安全地克隆（`clone`）并在多个线程之间共享，克隆操作非常轻量（仅增加 `Arc` 的引用计数）。

## Snowflake ID 结构

生成的 ID 是一个 64 位整数（`u64`），其结构如下（默认配置）：

```text
┌─────────────────────────────────────────────────────────────────┐
│ 0 │ 41 bits: 时间戳  │ 5 bits: 数据中心 │ 5 bits: 机器 │ 12 bits: 序列号 │
└─────────────────────────────────────────────────────────────────┘
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
snowflake-me = "1.0"
```

启用 IP 地址自动回退功能：

```toml
[dependencies]
snowflake-me = { version = "2.0", features = ["ip-fallback"] }
```

一次性启用所有可选特性：

```toml
[dependencies]
snowflake-me = { version = "2.0", features = ["full"] }
```

### 特性标志

| 特性 | 默认 | 描述 |
|------|------|------|
| `std` | 是 | 标准库支持（通过 `jiff` 获取时间）。在 `no_std` 环境下请禁用。 |
| `ip-fallback` | 否 | 从本地网络接口（IPv4/IPv6）自动检测 `machine_id` 和 `data_center_id`。需要 `std`。 |
| `serde` | 否 | `SnowflakeId`（u64）和 `SnowflakeIdString`（字符串）的 Serde 序列化/反序列化支持。 |
| `tracing` | 否 | 通过 `tracing` 在关键路径（ID 生成、时钟漂移等）输出结构化日志。 |
| `metrics` | 否 | 通过 `metrics` 提供计数器和仪表盘指标，用于可观测性。 |
| `use-strong-cas` | 否 | 使用 `compare_exchange` 替代 `compare_exchange_weak`。略慢但消除伪 CAS 失败。 |
| `full` | 否 | 一次性启用所有可选特性。 |

### 2. 基本用法

```rust
# #[cfg(feature = "std")] {
use snowflake_me::Snowflake;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 使用 Builder 显式配置机器 ID 和数据中心 ID。
    // 或者，启用 `ip-fallback` 特性并使用 `Snowflake::new()`
    // 从本地网络接口自动获取 ID。
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    // 生成一个唯一的 ID
    let id = sf.next_id()?;
    println!("Generated Snowflake ID: {id}");

    Ok(())
}
# }
```

### 3. 批量生成

单次调用生成多个唯一 ID：

```rust
# #[cfg(feature = "std")] {
use snowflake_me::Snowflake;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .finalize()?;

    let ids = sf.next_ids(100)?;
    println!("生成了 {} 个 ID", ids.len());

    Ok(())
}
# }
```

### 4. 多线程用法

`Snowflake` 实例可以被高效地克隆并在线程间共享。

```rust
# #[cfg(feature = "std")] {
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
            assert!(all_ids.insert(id), "发现重复 ID: {id}");
        }
    }

    println!("成功在 10 个线程中生成了 {} 个唯一 ID", all_ids.len());
    Ok(())
}
# }
```

### 5. 分解 ID

您可以将一个 Snowflake ID 分解回其组成部分，以进行调试或分析。

```rust
# #[cfg(feature = "std")] {
use snowflake_me::Snowflake;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(15))
        .data_center_id(&|| Ok(7))
        .finalize()?;

    let id = sf.next_id()?;
    let decomposed = sf.decompose(id);

    println!("ID: {}", decomposed.id);
    println!("时间: {}", decomposed.time);
    println!("数据中心 ID: {}", decomposed.data_center_id);
    println!("机器 ID: {}", decomposed.machine_id);
    println!("序列号: {}", decomposed.sequence);

    assert_eq!(decomposed.machine_id, 15);
    assert_eq!(decomposed.data_center_id, 7);

    Ok(())
}
# }
```

### 6. 时钟漂移保护

如果系统时钟发生回退（例如 NTP 调整），生成器会根据配置的策略进行处理。默认情况下，会忙等待直到时钟恢复。

```rust
# #[cfg(feature = "std")] {
use snowflake_me::{Snowflake, ClockDriftStrategy};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .clock_drift_strategy(ClockDriftStrategy::Wait)
        .max_clock_drift_ms(5000)  // 漂移超过 5 秒则报错
        .finalize()?;

    let id = sf.next_id()?;
    println!("Generated ID: {id}");

    Ok(())
}
# }
```

可用策略：
- **`ClockDriftStrategy::Wait`**（默认）— 忙等待直到时钟恢复。可设置 `max_clock_drift_ms` 在漂移过大时返回错误。
- **`ClockDriftStrategy::Error`** — 检测到时钟回退时立即返回 `Error::ClockDrift`。
- **`ClockDriftStrategy::LastTimestamp`** — 复用上次已知的时间戳。ID 仍然唯一，但时间戳变为近似值。

### 7. `no_std` 用法

在 `no_std` 环境下，禁用默认特性并提供时间源：

```toml
[dependencies]
snowflake-me = { version = "2.0", default-features = false }
```

```rust,ignore
use snowflake_me::{Snowflake, set_time_source};

// 周期性调用（例如在定时器中断或 RTC 读取中）
set_time_source(get_current_millis());

let sf = Snowflake::builder()
    .start_time(1_640_995_200_000) // 2022-01-01 UTC，毫秒时间戳
    .machine_id(&|| Ok(1))
    .data_center_id(&|| Ok(1))
    .finalize()
    .unwrap();

let id = sf.next_id().unwrap();
```

## 从 v0.6.x 迁移

如果您从 v0.6.x 升级，请注意以下破坏性变更：

- `next_id()` 现在返回 `Result<SnowflakeId, Error>` 而非 `Result<u64, Error>`。使用 `id.as_u64()` 获取原始 `u64` 值。
- `DecomposedSnowflake::id` 字段类型从 `u64` 变为 `SnowflakeId`。
- `Error::NoPrivateIPv4` 重命名为 `Error::NoPrivateIP`（同时尝试 IPv6 回退）。
- `Error::MutexPoisoned` 已移除（生成器现在是无锁的）。
- 新增可选特性：`serde`、`tracing`、`metrics`、`use-strong-cas`。

## 贡献

欢迎提交问题（Issues）和拉取请求（Pull Requests）。

## 许可证

本项目采用 [MIT](LICENSE-MIT) 和 [Apache 2.0](LICENSE-APACHE) 双重许可证。
