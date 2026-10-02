# Snowflake-Me

[English](README.md) | 简体中文

[![Crates.io](https://img.shields.io/crates/v/snowflake_me.svg)](https://crates.io/crates/snowflake_me)
[![Docs.rs](https://docs.rs/snowflake_me/badge.svg)](https://docs.rs/snowflake_me)
[![Build Status](https://github.com/houseme/snowflake-rs/workflows/Build/badge.svg)](https://github.com/houseme/snowflake-rs/actions?query=workflow%3ABuild)
[![License](https://img.shields.io/crates/l/snowflake_me)](LICENSE-APACHE)
[![Miri](https://github.com/houseme/snowflake-rs/actions/workflows/miri.yml/badge.svg)](https://github.com/houseme/snowflake-rs/actions/workflows/miri.yml)
[![Semver](https://github.com/houseme/snowflake-rs/actions/workflows/semver.yml/badge.svg)](https://github.com/houseme/snowflake-rs/actions/workflows/semver.yml)

一个高性能、高并发、分布式的 Rust Snowflake ID 生成器实现。

ID 分配采用 **无锁 CAS 循环**。序列耗尽或时钟回退时，调用仍可能等待；`std` 环境下可以配置等待超时。

## 设计亮点

- **无锁并发**：使用 `AtomicU64` 和 CAS (Compare-And-Swap) 操作来管理内部状态，分配路径不使用互斥锁。
- **高性能**：缓存行对齐（`#[repr(align(64))]`）减少相邻状态的共享；多个线程仍会竞争同一个原子状态。默认使用 `compare_exchange_weak`，两种 CAS 模式的性能需要在目标硬件上比较。
- **高度可定制**：通过 `Builder` 模式，您可以灵活配置以下参数：
    - `start_time`: 起始时间戳，用于缩短生成 ID 的时间部分。
    - `machine_id` 和 `data_center_id`: 机器和数据中心标识符。
    - 各部分位长 (`time`, `sequence`, `machine_id`, `data_center_id`)。
    - 时钟漂移策略和最大允许漂移量。
- **批量生成**：通过 `next_ids(count)` 单次调用生成多个唯一 ID，分摊调用开销。
- **智能 IP 地址兜底**：启用 `ip-fallback` 特性后，如果未提供 `machine_id` 或 `data_center_id`，系统会自动从本机网络接口获取。
    - **同时支持 IPv4 和 IPv6**：优先使用私有 IPv4 地址，若无则回退到私有 IPv6 地址。
    - **部署约束**：从不同地址部分派生并按配置位宽截断，不同 IP 仍可能得到相同节点标识；IP 回退不会协调节点 ID 分配。
- **`no_std` 支持**：支持 `no_std` + `alloc` 环境，由用户提供时间源。
- **线程安全**：`Snowflake` 实例可以被安全地克隆（`clone`）并在多个线程之间共享，克隆操作非常轻量（仅增加 `Arc` 的引用计数）。

## Snowflake ID 结构

生成的 ID 是一个 64 位整数（`u64`），其结构如下（默认配置）：

```text
┌─────────────────────────────────────────────────────────────────┐
│ 0 │ 41 bits: 时间戳  │ 5 bits: 数据中心 │ 5 bits: 机器 │ 12 bits: 序列号 │
└─────────────────────────────────────────────────────────────────┘
```

- **符号位 (1 bit)**: 始终为 0，生成的 ID 可表示为非负 `i64`。epoch 时刻、节点标识和序列均为零时，ID 可以为零。
- **时间戳 (41 bits)**: 从您设定的 `start_time` 开始的毫秒数。41 位可以表示约 69 年的时间。
- **数据中心 ID (5 bits)**: 允许最多 32 个数据中心。
- **机器 ID (5 bits)**: 每个数据中心允许最多 32 台机器。
- **序列号 (12 bits)**: 表示在同一毫秒内，一台机器上可以生成的 ID 数量。12 位允许每毫秒生成 4096 个 ID。

**注意**：各部分位宽总和必须为 63。机器和数据中心字段分别支持 0–16 位；零位字段的 ID 必须为零。构造生成器前会拒绝非法布局。

## 唯一性和容量

同一个 `(data_center_id, machine_id)` 应共享一个生成器或它的克隆。独立构造的生成器不共享状态：两个 `Snowflake::default()` 都使用 `(0, 0)`，可能生成重复 ID。生产环境中，使用相同 epoch 和位布局的独立生成器必须分配不同的节点标识。

状态只保存在内存中。重启后复用节点标识时，需要确保时钟晚于前一个生成器最后发号的时间戳，或通过外部协调避免并发复用和时间戳复用。IP 回退只用于简化配置：默认位宽下，`192.168.1.1` 与 `192.168.33.33` 都映射为 `(1, 1)`。

默认 12 位序列每毫秒可发出 4096 个 ID；时钟正常推进时，每个节点标识的持续容量约为每秒 409.6 万个 ID。批量生成降低原子操作和内存分配开销，但不会提高这个容量。增加序列位数会压缩时间范围或节点空间，改变布局需要安排迁移。详见[基准测试方法](docs/BENCHMARKS.md)。

## 快速开始

### 1. 添加依赖

库支持 Rust 1.88 及以上版本；开发测试和基准使用当前 stable 工具链。`main` 源码可能包含尚未发布的 API，详见 [CHANGELOG.md](CHANGELOG.md)。

将此库添加到您的 `Cargo.toml` 文件中：

```toml
[dependencies]
snowflake_me = "2.1.1"
```

启用 IP 地址自动回退功能：

```toml
[dependencies]
snowflake_me = { version = "2.1.1", features = ["ip-fallback"] }
```

一次性启用所有可选特性：

```toml
[dependencies]
snowflake_me = { version = "2.1.1", features = ["full"] }
```

### 特性标志

| 特性 | 默认 | 描述 |
|------|------|------|
| `std` | 是 | 标准库支持（通过 `jiff` 获取时间）。在 `no_std` 环境下请禁用。 |
| `ip-fallback` | 否 | 从本地网络接口（IPv4/IPv6）自动检测 `machine_id` 和 `data_center_id`。需要 `std`。 |
| `serde` | 否 | `SnowflakeId`（u64）和 `SnowflakeIdString`（字符串）的 Serde 序列化/反序列化支持。 |
| `tracing` | 否 | 在关键路径输出结构化日志，需要 `std`。 |
| `metrics` | 否 | 提供计数器和仪表盘指标，需要 `std`。 |
| `use-strong-cas` | 否 | 使用 `compare_exchange` 替代 `compare_exchange_weak`。消除伪 CAS 失败；性能取决于硬件和负载。 |
| `full` | 否 | 启用 `std`、`serde`、`tracing`、`metrics`、`ip-fallback`，保留 weak CAS。 |

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

需要复用内存时，可向已有向量追加 ID。发生错误后，已追加的 ID 仍然有效且不会重新分配；调用前后的长度差表示已生成数量：

```rust
use snowflake_me::Snowflake;
let sf = Snowflake::builder()
    .start_time(0)
    .clock_source(|| 1_000)
    .machine_id(&|| Ok(1))
    .data_center_id(&|| Ok(1))
    .finalize()?;
let mut ids = Vec::with_capacity(100);
sf.next_ids_into(100, &mut ids)?;
ids.clear(); // 下一批继续复用内存。
sf.next_ids_into(100, &mut ids)?;
# Ok::<(), snowflake_me::Error>(())
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

如果系统时钟发生回退（例如 NTP 调整），生成器会根据配置的策略进行处理。默认等待直到时钟恢复；`std` 下短暂自旋后会让出线程。未配置等待超时时，停滞的时钟可能导致无限等待。

```rust
# #[cfg(feature = "std")] {
use snowflake_me::{Snowflake, ClockDriftStrategy};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sf = Snowflake::builder()
        .machine_id(&|| Ok(1))
        .data_center_id(&|| Ok(1))
        .clock_drift_strategy(ClockDriftStrategy::Wait)
        .max_clock_drift_ms(5000)  // 漂移超过 5 秒则报错
        .wait_timeout(std::time::Duration::from_millis(100))
        .finalize()?;

    let id = sf.next_id()?;
    println!("Generated ID: {id}");

    Ok(())
}
# }
```

可用策略：
- **`ClockDriftStrategy::Wait`**（默认）— 等待直到时钟恢复。`max_clock_drift_ms` 必须非负，等待期间会持续检查，包括序列耗尽引发的等待。
- **`ClockDriftStrategy::Error`** — 检测到时钟回退时立即返回 `Error::ClockDrift`。
- **`ClockDriftStrategy::LastTimestamp`** — 仍有可用序列号时复用上次时间戳。序列耗尽后仍需等待时钟推进，不能在持续回拨期间无限发号。

`std` 下可以用 `wait_timeout(Duration)`，通过单调时钟限制单次生成调用中的等待期间。等待期限与允许的墙上时钟回拨幅度是两个独立限制。超时返回 `Error::WaitTimeout`；无法按配置策略处理的 epoch 之前时间返回 `Error::BeforeEpoch`。

确定性测试或设备时钟可以通过 `Builder::clock_source(F)` 注入返回 Unix 毫秒数的线程安全闭包。克隆共享该时钟，独立构造的生成器可使用不同时间源。默认仍使用系统时钟（`std`）或 `set_time_source`（无 `std`）。

### 7. `no_std` 用法

在 `no_std` 环境下，禁用默认特性并提供时间源。需要分配器和原生 64 位原子操作；CI 检查 `aarch64-unknown-none`，包括可选的 `serde`。调用等待时，时钟必须能够独立推进，例如由中断更新时间；只在调用返回后更新时间无法解除序列耗尽等待：

```toml
[dependencies]
snowflake_me = { version = "2.1.1", default-features = false }
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
