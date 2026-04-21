# 部署与运维

## 二进制部署

1. 在目标机器安装兼容的 libc（Linux 常见为 glibc，与构建机一致可减少问题）。
2. `cargo build --release`，将 `target/release/execgo-runtime` 拷贝到 `PATH` 中。
3. 使用 systemd、supervisor 或容器编排启动 `serve`，并持久化 `--data-dir`。

建议：

- 仅内网或通过反向代理暴露；API 当前无内置认证，需在网络层或网关层做鉴权。
- 磁盘：为 `data-dir` 预留足够空间用于日志与任务产物。
- 备份：定期备份 `runtime.db` 与业务需要的 `tasks/` 子目录。

## 健康检查

- **存活**：`GET /healthz`
- **就绪**：`GET /readyz`（验证存储可用）
- **能力清单**：`GET /api/v1/runtime/capabilities`
- **资源快照**：`GET /api/v1/runtime/resources`

编排时可配置：

- liveness → `/healthz`
- readiness → `/readyz`

## 指标与监控

`GET /metrics` 提供 Prometheus 文本。可在 Prometheus 中抓取该路径，或交给 Datadog/VictoriaMetrics 等兼容端点。

若上层 ExecGo 需要根据宿主能力做调度/策略决策，建议同时拉取：

- `/api/v1/runtime/info`
- `/api/v1/runtime/capabilities`
- `/api/v1/runtime/config`
- `/api/v1/runtime/resources`

## Docker 示例

仓库提供 `Dockerfile`（多阶段构建），并通过 GitHub Actions 发布到 GitHub Container Registry（GHCR）。

拉取已发布镜像：

```bash
docker pull ghcr.io/iammm0/execgo-runtime:latest
docker run --rm -p 8080:8080 -v execgo-data:/data ghcr.io/iammm0/execgo-runtime:latest
```

本地快速原型用法：

```bash
docker build -t execgo-runtime:local .
docker run --rm -p 8080:8080 -v execgo-data:/data execgo-runtime:local
```

镜像入口为 `serve`，监听 `0.0.0.0:8080`，数据目录 `/data`。

## CI/CD

本仓库使用 **GitHub Actions**：

- **CI**（`.github/workflows/ci.yml`）：在 push / PR 上执行 `fmt`、`clippy`、`test`。
- **Container image**（`.github/workflows/container.yml`）：在 `main` / `master` 或版本标签 push 后构建 Docker 镜像并推送到 `ghcr.io/iammm0/execgo-runtime`。
- **Release 构建**（`.github/workflows/release.yml`）：在推送以数字开头的版本标签（如 `1.0.0-b1`）时构建 Linux/macOS  release 二进制并上传 Artifact（预发布/验证用）。

流水线定义以仓库内 YAML 为准。

GHCR 标签策略：

- 默认分支构建：`latest`、`main`（或对应分支名）、`sha-<commit>`。
- 版本标签构建：原始 Git 标签（如 `1.0.0-b1`）和 `sha-<commit>`。

## 版本与标签

- **Cargo 版本**：与 `Cargo.toml` 中 `version` 一致，`/healthz` 中 `version` 字段来自该值。
- **Git 标签**：发布节点可打标签（如 `1.0.0-b1`），便于对照源码与二进制产物。

预发布版本（`-b1`、`-beta` 等）表示 API 与行为仍可能调整；升级前请阅读变更说明。

## ExecGo 环境变量

在 ExecGo 控制面配置：

```text
EXECGO_RUNTIME_URL=http://<host>:<port>
```

指向本服务根 URL（无尾部 `/` 亦可，客户端会裁剪）。
