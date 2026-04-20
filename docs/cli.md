# 命令行参考

入口：`execgo-runtime`（或由 `cargo run --` 调用）。

全局行为：通过子命令选择模式；`serve` 启动服务，其余子命令为**客户端**，使用 HTTP 访问远程服务。

## `serve`

启动 HTTP 服务。

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--listen-addr` | `127.0.0.1:8080` | 监听地址。生产环境常设为 `0.0.0.0:8080` 并由反向代理 TLS 终结。 |
| `--data-dir` | `data` | 数据根目录（SQLite、任务目录）。 |
| `--max-running-tasks` | `4` | 并发运行任务上限。 |
| `--max-queued-tasks` | `128` | 队列中 `accepted` 任务上限；超出返回 429。 |
| `--termination-grace-ms` | `5000` | kill/超时后 SIGTERM 到 SIGKILL 的等待。 |
| `--result-retention-secs` | `604800` | 终态任务结果保留时间（约 7 天），供 GC 使用。 |
| `--gc-interval-ms` | `1000` | 垃圾回收轮询间隔。 |
| `--dispatch-poll-interval-ms` | `250` | 调度器在无通知时的轮询间隔（与内部 `Notify` 配合）。 |
| `--cgroup-root` | `/sys/fs/cgroup/execgo-runtime` | Linux cgroup v2 挂载点下用于每任务子目录（`linux_sandbox` 时）。 |

日志：默认 `tracing` JSON，可通过环境变量 `RUST_LOG` 调整（如 `RUST_LOG=info`）。

## `submit`

提交任务。必须提供 `--json` 或 `--file` 之一。

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--server` | `http://127.0.0.1:8080` | 服务根 URL。 |
| `--json` | — | 请求 JSON 字符串。 |
| `--file` | — | 请求 JSON 文件路径。 |
| `--poll-interval-ms` | `500` | 供其他子命令复用字段；`submit` 本身仅 POST。 |
| `--timeout-ms` | — | 同上，对 `submit` 无等待语义。 |

成功时向标准输出打印响应 JSON。

## `status`

查询任务：`execgo-runtime status <task_id>`。

| 参数 | 说明 |
|------|------|
| `--server` | 同 `submit`。 |
| `task_id` | 位置参数。 |

## `wait`

阻塞轮询直到任务进入终态。

| 参数 | 说明 |
|------|------|
| `--server` | 服务根 URL。 |
| `task_id` | 任务 ID。 |
| `--timeout-ms` | 可选；超时则进程以非零退出并打印错误。 |
| `--poll-interval-ms` | 默认 `500`。 |

## `kill`

`execgo-runtime kill <task_id>`，语义同 API `POST .../kill`。

## `run`

等价于 **提交 + `wait`**：先 `POST /api/v1/tasks`，再对返回的 `task_id` 执行与 `wait` 相同的轮询逻辑。

适合脚本中「跑完一条任务并拿到最终 JSON」。

## `internal-shim`（隐藏）

由运行时**自动** fork，不应手工用于正常运维。用于从数据库加载任务并执行用户进程。

---

## 示例

```bash
# 前台启动（开发）
execgo-runtime serve --listen-addr 127.0.0.1:8080 --data-dir ./data

# 从文件提交
execgo-runtime submit --server http://127.0.0.1:8080 --file ./task.json

# 同步执行至结束
execgo-runtime run --json '{"execution":{"kind":"command","program":"/bin/sh","args":["-c","date"]}}'
```
