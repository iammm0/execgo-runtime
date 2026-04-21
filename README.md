# execgo-runtime

[![CI](https://github.com/iammm0/execgo-runtime/actions/workflows/ci.yml/badge.svg)](https://github.com/iammm0/execgo-runtime/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.74%2B-orange.svg)](https://www.rust-lang.org/)
[![Crates.io](https://img.shields.io/crates/v/execgo-runtime.svg)](https://crates.io/crates/execgo-runtime)

ExecGo 生态中的**数据面运行时**：用 Rust 实现任务的异步提交、调度、执行与持久化，对外提供 **HTTP API** 与 **CLI**，可作为 ExecGo 控制面背后的执行后端。

**当前版本**：`1.0.0-b1`（预发布，行为与 API 仍可能调整；详见 [版本与标签](docs/deployment.md#版本与标签)）。

---

## 目录

- [简介](#简介)
- [功能特性](#功能特性)
- [环境要求](#环境要求)
- [安装](#安装)
- [快速开始](#快速开始)
- [配置与环境变量](#配置与环境变量)
- [HTTP API 概览](#http-api-概览)
- [与 ExecGo 集成](#与-execgo-集成)
- [文档](#文档)
- [参与贡献](#参与贡献)
- [安全](#安全)
- [许可证](#许可证)

---

## 简介

`execgo-runtime` 在单进程中托管 HTTP 服务，通过 SQLite（WAL）与任务目录持久化状态；调度器将队列中的任务派发到 **internal shim** 子进程执行用户命令或脚本。runtime 采用“单一版本、多能力面”的设计：启动时探测宿主环境，暴露 capability manifest，并在任务提交时解析 requested/effective execution plan。支持健康检查、就绪探针、Prometheus 指标、任务取消与超时控制、本机资源账本，以及 Linux 上可选的 `linux_sandbox` 与 cgroup 能力（详见 [架构说明](docs/architecture.md)）。

## 功能特性

| 能力 | 说明 |
|------|------|
| HTTP API | 提交任务、查询状态、取消、事件流、健康检查、Prometheus 指标 |
| CLI | `serve` / `submit` / `status` / `wait` / `kill` / `run` |
| 持久化 | SQLite 元数据；`tasks/<id>/` 下 `request.json`、`result.json`、stdout/stderr |
| 调度与恢复 | shim 子进程执行；重启后可对非终态任务做恢复相关处理 |
| 自适应能力 | 环境探测、capability API、显式降级、requested/effective execution plan |
| 资源与沙箱 | 默认进程级；Linux 可选 `linux_sandbox`（命名空间、cgroup 等）；本机 ResourceLedger 做 reservation/release |

## 环境要求

| 项目 | 说明 |
|------|------|
| Rust | **1.74+**（MSRV；建议当前 stable，与 CI 一致。`edition = "2021"` 表示语言 edition，不是 Rust 工具链版本） |
| 操作系统 | 开发与 CI 覆盖 **Linux、macOS**；**沙箱 / cgroup 完整能力仅在 Linux** |
| 平台 | 依赖 Unix 进程组、信号、`wait4` 等；**Windows 非目标平台** |

## 安装

### 从 crates.io 安装

```bash
cargo install execgo-runtime --version 1.0.0-b1
```

### 从源码构建（推荐）

```bash
git clone https://github.com/iammm0/execgo-runtime.git
cd execgo-runtime
cargo build --release
# 二进制：target/release/execgo-runtime
```

### 容器镜像（可选）

从 GitHub Container Registry 拉取（`main` 分支成功构建后可用）：

```bash
docker pull ghcr.io/iammm0/execgo-runtime:latest
docker run --rm -p 8080:8080 -v execgo-data:/data ghcr.io/iammm0/execgo-runtime:latest
```

或从源码本地构建：

```bash
docker build -t execgo-runtime:local .
docker run --rm -p 8080:8080 -v execgo-data:/data execgo-runtime:local
```

默认监听 `0.0.0.0:8080`，数据目录 `/data`。详见 [部署说明](docs/deployment.md)。

## 快速开始

### 编译与测试

```bash
cargo build --release
cargo test
```

### 启动服务

```bash
cargo run -- serve --listen-addr 127.0.0.1:8080 --data-dir ./data
```

数据目录下会生成 `runtime.db` 与 `tasks/<task_id>/`。

### 提交示例任务

```bash
cargo run -- submit --json '{"execution":{"kind":"command","program":"/bin/sh","args":["-c","echo hello"]}}'
```

### 一键演示

```bash
chmod +x scripts/quickstart.sh
./scripts/quickstart.sh
```

在临时目录启动服务、提交任务并等待结束，退出时清理。

### CLI 速查

| 子命令 | 作用 |
|--------|------|
| `serve` | 启动 HTTP 服务（`serve --help` 查看全部参数） |
| `submit` | 提交任务（`--json` 或 `--file`） |
| `status <task_id>` | 查询状态 |
| `wait <task_id>` | 轮询至终态（可选 `--timeout-ms`） |
| `kill <task_id>` | 请求取消 |
| `run` | 提交并等待完成（`submit` + `wait`） |

默认 `--server http://127.0.0.1:8080`。

## 配置与环境变量

| 变量 | 说明 |
|------|------|
| `RUST_LOG` | 日志级别，如 `info`、`debug`；未设置时由程序默认 `tracing` 配置 |
| `EXECGO_RUNTIME_URL` | **在 ExecGo 控制面**配置：指向本服务根 URL（无尾斜杠亦可） |
| `EXECGO_RUNTIME_ID` | 可选：覆盖 runtime 节点 ID |
| `EXECGO_RUNTIME_DEFAULT_CAPABILITY_MODE` | 可选：默认 capability 策略，`adaptive` 或 `strict` |
| `EXECGO_RUNTIME_DISABLE_LINUX_SANDBOX` | 可选：禁用 Linux sandbox 探测能力 |
| `EXECGO_RUNTIME_DISABLE_CGROUP` | 可选：禁用 cgroup 能力 |
| `EXECGO_RUNTIME_CAPACITY_MEMORY_BYTES` / `EXECGO_RUNTIME_CAPACITY_PIDS` | 可选：覆盖 ResourceLedger 容量探测 |

服务端常用参数见 [CLI 文档](docs/cli.md)（并发上限、队列长度、GC、grace 时间等）。

## HTTP API 概览

| 方法 | 路径 | 说明 |
|------|------|------|
| `POST` | `/api/v1/tasks` | 提交任务 |
| `GET` | `/api/v1/tasks/:id` | 任务状态（含输出片段） |
| `POST` | `/api/v1/tasks/:id/kill` | 取消 |
| `GET` | `/api/v1/tasks/:id/events` | 事件列表 |
| `GET` | `/api/v1/runtime/info` | runtime ID、版本、启动时间与平台摘要 |
| `GET` | `/api/v1/runtime/capabilities` | 宿主环境探测结果、基础语义、增强能力、降级告警 |
| `GET` | `/api/v1/runtime/config` | 非敏感运行配置摘要 |
| `GET` | `/api/v1/runtime/resources` | 本机资源 capacity/reserved/available 与活动 reservation |
| `GET` | `/healthz` | 存活（响应中含 `version`） |
| `GET` | `/readyz` | 就绪（校验存储） |
| `GET` | `/metrics` | Prometheus 文本 |

完整 JSON 模型与示例见 [API 参考](docs/api.md)。

## 与 ExecGo 集成

在 ExecGo 中设置环境变量：

```text
EXECGO_RUNTIME_URL=http://127.0.0.1:8080
```

指向正在运行的 `execgo-runtime` 根地址；控制面通过该 URL 调用上述 API。

## 文档

| 文档 | 内容 |
|------|------|
| [docs/README.md](docs/README.md) | 文档索引 |
| [docs/architecture.md](docs/architecture.md) | 架构与执行流程 |
| [docs/api.md](docs/api.md) | HTTP API 与 JSON |
| [docs/cli.md](docs/cli.md) | 命令行参数 |
| [docs/deployment.md](docs/deployment.md) | 部署、Docker、CI/CD、标签 |
| [docs/development.md](docs/development.md) | 本地开发、测试、风格 |

## 参与贡献

欢迎通过 [Issues](https://github.com/iammm0/execgo-runtime/issues) 反馈缺陷与需求，通过 Pull Request 提交改动。

建议流程：

1. Fork 本仓库，从 `main` 创建分支。
2. 本地执行 `cargo fmt`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test`。
3. 提交信息清晰说明**动机与行为变化**（可与 [development.md](docs/development.md) 对齐）。
4. 发起 PR，等待 CI 通过后再合并。

## 安全

若你认为发现了安全漏洞，请**不要**在公开 Issue 中讨论；请通过仓库维护者提供的私密渠道报告（可在 GitHub 用户主页或组织说明中查找联系方式）。在未有 `SECURITY.md` 前，也可先开 **私有** 沟通渠道联系维护者。

## 许可证

本项目使用 `MIT` 许可证发布，完整条款见仓库根目录的 `LICENSE` 文件。

---

**仓库**：<https://github.com/iammm0/execgo-runtime>
