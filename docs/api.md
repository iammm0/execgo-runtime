# HTTP API 参考

基路径：服务根 URL（例如 `http://127.0.0.1:8080`）。以下路径均为相对该根路径。

**Content-Type**：请求与响应 JSON 使用 `application/json`（由 Axum JSON 提取器处理）。

## 通用错误格式

失败时返回 JSON：

```json
{
  "error": {
    "code": "invalid_input",
    "message": "人类可读说明",
    "details": null
  }
}
```

`code` 与 `types::ErrorCode` 对应，序列化为 `snake_case`（如 `invalid_input`、`queue_full` 映射为 `resource_limit_exceeded` 等，见服务端 `AppError` 映射）。
自适应运行时新增 `unsupported_capability` 与 `insufficient_resources`，分别表示 strict 策略下能力不可满足，以及单任务资源请求超过本 runtime 的本机 capacity。

常见 HTTP 状态码：

| HTTP | 含义 |
|------|------|
| 400 | 输入非法（如校验失败） |
| 404 | 任务不存在 |
| 409 | 冲突（如 `task_id` 已存在） |
| 429 | 队列已满（`max_queued_tasks`） |
| 500 | 内部错误 |

---

## POST `/api/v1/tasks`

提交任务。

### 请求体：`SubmitTaskRequest`

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `task_id` | string | 否 | 自定义 ID；若省略则服务端生成 UUID。仅允许字母、数字、`-`、`_`、`.`。 |
| `execution` | object | 是 | 见 [ExecutionSpec](#executionspec)。 |
| `limits` | object | 否 | 见 [ResourceLimits](#resourcelimits)，有默认值。 |
| `sandbox` | object | 否 | 见 [SandboxPolicy](#sandboxpolicy)，默认 `process`。 |
| `policy` | object | 否 | 见 [TaskPolicy](#taskpolicy)，默认使用服务端 `default_capability_mode`（通常为 `adaptive`）。 |
| `control_context` | object | 否 | 上层 ExecGo 显式传入的控制面上下文；runtime 不主动猜测控制面环境。 |
| `metadata` | object | 否 | 字符串到字符串的 map（有序序列化为对象）。 |

### ExecutionSpec

| 字段 | 类型 | 说明 |
|------|------|------|
| `kind` | `"command"` \| `"script"` | 执行模式。 |
| `program` | string | `command` 时：可执行文件路径。 |
| `args` | string[] | 命令参数。 |
| `script` | string | `script` 时：脚本内容。 |
| `interpreter` | string[] | 可选；如 `["python3"]` 或 `["bash","-lc"]`。 |
| `env` | object | 额外环境变量（键不可含 `=`）。 |

**注意**：`command` 与 `script` 互斥字段规则见 `ExecutionSpec::validate`。

### ResourceLimits

| 字段 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `wall_time_ms` | number | 300000 | 墙钟超时；超时后发送 SIGTERM，grace 后 SIGKILL。 |
| `cpu_time_sec` | number | 可选 | 对应 `RLIMIT_CPU`。 |
| `memory_bytes` | number | 可选 | 对应 `RLIMIT_AS`；Linux 沙箱下可与 cgroup 协同。 |
| `pids_max` | number | 可选 | Linux cgroup `pids.max`（沙箱路径下）。 |
| `stdout_max_bytes` | number | 4194304 | 状态查询中内联返回的 stdout 最大字节。 |
| `stderr_max_bytes` | number | 4194304 | 同上，stderr。 |

### SandboxPolicy

| 字段 | 类型 | 说明 |
|------|------|------|
| `profile` | `"process"` \| `"linux_sandbox"` | 非 Linux 仅允许 `process`。 |
| `workspace_subdir` | string | 相对 `workspace` 的子目录，禁止 `..` 与绝对路径。 |
| `rootfs` | string | `chroot` 时根文件系统路径。 |
| `chroot` | bool | 仅 `linux_sandbox` 与 `rootfs` 组合合法。 |
| `namespaces` | object | 可选；各字段控制是否 unshare 对应命名空间（见源码 `NamespaceConfig`）。 |

### TaskPolicy

| 字段 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `capability_mode` | `"adaptive"` \| `"strict"` | 服务端默认 | `adaptive` 允许显式降级并在 `execution_plan` 中说明；`strict` 在能力不可满足时返回 `unsupported_capability`。 |

### ControlContext

| 字段 | 类型 | 说明 |
|------|------|------|
| `control_plane_mode` | string | 上层 ExecGo 运行模式描述。 |
| `tenant` | string | 可选租户标识；当前 runtime 不做租户配额。 |
| `expected_runtime_profile` | string | 上层期望的 runtime profile。 |
| `requires_strict_sandbox` | bool | 若为 true，runtime 会把 sandbox 能力降级视为 strict 拒绝。 |
| `requires_resource_reservation` | bool | 记录上层是否要求资源 reservation；本版本 ResourceLedger 始终启用。 |
| `labels` | object | 字符串标签。 |

### 响应：`SubmitTaskResponse`

| 字段 | 说明 |
|------|------|
| `task_id` | 任务 ID。 |
| `handle_id` | 当前实现与 `task_id` 相同。 |
| `status` | 初始为 `accepted`。 |

---

## GET `/api/v1/tasks/:id`

查询任务状态与输出摘要。

### 响应：`TaskStatusResponse`

主要字段：

- `task_id`, `handle_id`, `status`
- `created_at`, `updated_at`, `started_at`, `finished_at`（RFC3339 UTC）
- `duration_ms`, `shim_pid`, `pid`, `pgid`, `exit_code`, `exit_signal`
- `stdout`, `stderr`：截断后的内联文本；`stdout_truncated` / `stderr_truncated`
- `error`（若存在）：`RuntimeErrorInfo`
- `usage`：`ResourceUsage`（时长、CPU、RSS、内存峰值等）
- `execution_plan`：requested/effective sandbox、资源 enforcement、是否降级、fallback reason 与 capability warning。
- `reservation`：任务实际占用的本机 `task_slots`、`memory_bytes`、`pids` reservation（终态后仍保留用于审计）。
- `artifacts`：磁盘路径（`task_dir`、`request_path`、`result_path` 等）
- `metadata`

---

## POST `/api/v1/tasks/:id/kill`

请求取消任务。若任务尚未执行则直接取消；若已运行则发送 SIGTERM 并可能在 grace 后 SIGKILL。

响应体同 `GET` 任务状态。

---

## GET `/api/v1/tasks/:id/events`

返回事件数组 `EventRecord[]`，按 `seq` 升序。

每条包含：`seq`, `task_id`, `event_type`, `timestamp`, `message`, `data`。

`event_type` 包括：`submitted`, `accepted`, `planned`, `degraded`, `resource_reserved`, `resource_released`, `started`, `kill_requested`, `timeout_triggered`, `finished`, `failed`, `cancelled`, `recovered` 等。

---

## GET `/api/v1/runtime/info`

返回 runtime ID、版本、启动时间、capability snapshot 版本与平台摘要。

---

## GET `/api/v1/runtime/capabilities`

返回启动时生成的 capability manifest，包含：

- `platform`：OS、arch、是否容器化、是否 Kubernetes。
- `execution`：command/script/process group 能力。
- `sandbox`：process、linux_sandbox、chroot、namespace 能力。
- `resources`：rlimit、cgroup、OOM 检测、ResourceLedger 与 capacity。
- `stable_semantics` / `enhanced_semantics`：基础语义与增强语义列表。
- `warnings`、`degraded`、`overrides`：探测告警、是否降级、运维覆盖来源。

---

## GET `/api/v1/runtime/config`

返回非敏感运行配置摘要：并发上限、队列上限、GC/dispatch interval、grace 时间、cgroup root、默认 capability 策略等。

---

## GET `/api/v1/runtime/resources`

返回本机 ResourceLedger 快照：

- `capacity`：`task_slots`、可选 `memory_bytes`、可选 `pids`。
- `reserved` / `available`：当前 reservation 与剩余 capacity。
- `active_reservations`：仍未释放的任务 reservation 列表。
- `accepted_waiting_tasks`：仍处于 accepted 且未持有活动 reservation 的任务数量。

---

## GET `/healthz`

存活探测。

```json
{ "status": "ok", "version": "1.0.0-b1" }
```

`version` 来自构建时的 `CARGO_PKG_VERSION`。

---

## GET `/readyz`

就绪探测：会尝试初始化/连接数据库。

```json
{ "status": "ready", "version": "1.0.0-b1" }
```

---

## GET `/metrics`

Prometheus 文本格式（`text/plain; version=0.0.4`）。指标名以 `execgo_runtime_` 为前缀。

---

## 示例请求

**最小命令任务：**

```bash
curl -sS -X POST "http://127.0.0.1:8080/api/v1/tasks" \
  -H "Content-Type: application/json" \
  -d '{"execution":{"kind":"command","program":"/bin/sh","args":["-c","echo hi"]}}'
```

**查询状态：**

```bash
curl -sS "http://127.0.0.1:8080/api/v1/tasks/<task_id>"
```
