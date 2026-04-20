# execgo-runtime 文档

本目录包含 `execgo-runtime` 的设计说明、API 与运维资料。建议按下列顺序阅读。

## 目录

1. **[architecture.md](architecture.md)** — 组件划分、任务状态机、调度与 shim、持久化与恢复。
2. **[api.md](api.md)** — REST 路径、HTTP 状态码、请求与响应 JSON 字段说明。
3. **[cli.md](cli.md)** — `execgo-runtime` 各子命令与常用参数。
4. **[deployment.md](deployment.md)** — 二进制部署、Docker 示例、CI/CD、版本与标签策略。
5. **[development.md](development.md)** — 本地构建、测试、代码风格与提交约定。

## 对外索引

- 仓库根目录 [README.md](../README.md) 提供项目概览与快速开始。
- 健康检查：`GET /healthz` 返回 `version` 字段，与 Cargo 包版本一致。
