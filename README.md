# execgo-runtime

`execgo-runtime` 是 ExecGo 生态中的**数据面运行时**：用 Rust 实现异步任务提交、调度、执行与持久化，对外提供 HTTP API 与配套 CLI，适合作为 ExecGo 控制面背后的执行后端。

**当前版本**：`1.0.0-b1`（预发布 beta，见 [发行说明](docs/deployment.md#版本与标签)）。

## 功能概览

| 能力 | 说明 |
|------|------|
| HTTP API | 提交任务、查询状态、取消、事件流、健康检查、Prometheus 指标 |
| CLI | `serve` / `submit` / `status` / `wait` / `kill` / `run`，封装远程 API |
| 持久化 | SQLite（WAL）存储任务元数据；任务目录下保存 `request.json`、`result.json`、stdout/stderr |
| 调度与恢复 | 独立 **shim** 子进程执行任务；运行时重启后可对 `running` 任务做恢复标记 |
| 资源与沙箱 | 进程级默认；Linux 上可选 `linux_sandbox`（命名空间、cgroup 等，见文档） |

## 环境要求

- **Rust**：1.74+（建议使用当前 stable）
- **操作系统**：开发与测试以 **macOS / Linux** 为主；**沙箱与 cgroup 相关能力仅在 Linux** 上完整可用
- **Unix**：进程组、信号、`wait4` 等依赖类 Unix 环境（Windows 未作为目标平台）

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

数据目录中会生成 `runtime.db`（SQLite）以及 `tasks/<task_id>/` 任务文件。

### 提交示例任务

```bash
cargo run -- submit --json '{"execution":{"kind":"command","program":"/bin/sh","args":["-c","echo hello"]}}'
```

### 一键演示脚本

```bash
chmod +x scripts/quickstart.sh
./scripts/quickstart.sh
```

脚本会临时目录起服务、提交任务并打印结果，退出时自动清理。

## CLI 速查

| 子命令 | 作用 |
|--------|------|
| `serve` | 启动 HTTP 服务（见 `serve --help`） |
| `submit` | 提交任务（`--json` 或 `--file`） |
| `status <task_id>` | 查询状态 |
| `wait <task_id>` | 轮询直到终态（可 `--timeout-ms`） |
| `kill <task_id>` | 请求取消 |
| `run` | 提交并等待完成（组合 `submit` + `wait`） |

默认服务地址：`--server http://127.0.0.1:8080`。

## HTTP 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| `POST` | `/api/v1/tasks` | 提交任务 |
| `GET` | `/api/v1/tasks/:id` | 任务状态（含输出片段） |
| `POST` | `/api/v1/tasks/:id/kill` | 取消 |
| `GET` | `/api/v1/tasks/:id/events` | 事件列表 |
| `GET` | `/healthz` | 存活探测（含版本） |
| `GET` | `/readyz` | 就绪（校验存储可用） |
| `GET` | `/metrics` | Prometheus 文本指标 |

请求/响应 JSON 模型见 [API 参考](docs/api.md)。

## 与 ExecGo 集成

在 ExecGo 侧配置环境变量 **`EXECGO_RUNTIME_URL`**，指向正在监听的 `execgo-runtime` 服务根地址（例如 `http://127.0.0.1:8080`）。控制面通过该 URL 调用上述 HTTP API。

## 文档目录

| 文档 | 内容 |
|------|------|
| [docs/README.md](docs/README.md) | 文档索引 |
| [docs/architecture.md](docs/architecture.md) | 架构与执行流程 |
| [docs/api.md](docs/api.md) | HTTP API 与 JSON 模型 |
| [docs/cli.md](docs/cli.md) | 命令行参数说明 |
| [docs/deployment.md](docs/deployment.md) | 部署、容器、CI/CD、版本标签 |
| [docs/development.md](docs/development.md) | 本地开发、测试、贡献约定 |

## 仓库与协作

- **源码**：<https://github.com/iammm0/execgo-runtime.git>
- 问题反馈与 PR 欢迎通过 GitHub 进行。

## 许可证

若仓库根目录包含 `LICENSE` 文件，以该文件为准；否则请向维护者确认授权条款。
