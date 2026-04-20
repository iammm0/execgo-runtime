# 架构说明

## 定位

`execgo-runtime` 是 ExecGo 的**执行后端（数据面）**：接收任务描述、落盘、调度执行，并通过 HTTP 暴露状态与运维接口。控制面（如 ExecGo 自身）通过 HTTP 与本服务交互，**不直接** fork 用户进程。

## 模块划分

| 模块 | 路径 | 职责 |
|------|------|------|
| `server` | `src/server.rs` | Axum 路由：`/api/v1/*`、`/healthz`、`/readyz`、`/metrics` |
| `runtime` | `src/runtime.rs` | 运行时核心：提交、查询、kill、dispatcher、GC、shim 入口、进程执行 |
| `repo` | `src/repo.rs` | SQLite 访问：任务表、事件表、指标聚合 |
| `types` | `src/types.rs` | 请求/响应与策略类型（执行规格、沙箱、限额） |
| `metrics` | `src/metrics.rs` | 将仓库快照渲染为 Prometheus 文本 |
| `cli` | `src/cli.rs` | 命令行解析 |
| `error` | `src/error.rs` | 错误类型与 HTTP 映射 |

## 任务状态机

任务在数据库中的 `status` 取值（JSON 中为 snake_case）：

- `accepted`：已入队，等待调度。
- `running`：已派发 shim，且（在 shim 内）进程已启动或即将启动。
- `success` / `failed` / `cancelled`：终态。

终态任务在 `limits` 与保留策略下可被 **GC** 删除（见 `serve` 的 `--result-retention-secs` 等参数）。

## 调度与 shim

1. **Dispatcher** 循环从队列中取 `accepted` 任务，在不超过 `max_running_tasks` 时派发。
2. 派发时以**当前可执行文件**再执行 `internal-shim` 子命令，传入 `--database`、`--data-dir`、`--task-id` 等。
3. **Shim** 读取任务记录，构建 `Command`/`Script` 执行，在 `pre_exec` 中设置进程组、`rlimit`，在 Linux 上可选应用 Linux 沙箱与 cgroup。
4. 主进程通过 `wait4` 等待子进程结束，并结合取消、超时、OOM 等条件写入 `CompletionUpdate`。

运行时重启后，`recover` 会扫描非终态任务：对 `running` 若 shim 仍在则标记恢复事件；否则标记为失败并落盘结果。

## 持久化布局

在 `--data-dir` 下：

- `runtime.db`：SQLite 数据库（WAL 模式）。
- `tasks/<task_id>/` 目录：
  - `request.json`：提交时的完整请求。
  - `result.json`：终态快照（与 API 状态结构一致）。
  - `stdout.log` / `stderr.log`：输出日志。
  - `workspace/` 或 `workspace/<subdir>/`：工作目录（由 `sandbox.workspace_subdir` 决定）。

## 沙箱与平台差异

- **`sandbox.profile = process`**（默认）：在普通进程环境中执行，依赖 `rlimit` 等限制。
- **`sandbox.profile = linux_sandbox`**：仅在 **Linux** 上合法；在非 Linux 主机上提交请求会在校验阶段拒绝。

详见 [api.md](api.md) 中的沙箱字段说明。

## 指标

`GET /metrics` 输出 Prometheus 文本指标，包括按状态任务数、错误码分布、以及基于历史 `duration_ms` 的直方图近似（实现见 `metrics.rs`）。
