# execgo-runtime 文档

本目录包含 `execgo-runtime` 的设计说明、API 与运维资料。建议按下列顺序阅读。

当前 runtime 已演进为“单一版本、多能力面”的自适应数据面运行时：启动时探测宿主环境，暴露 capability manifest，并通过 execution plan 与 ResourceLedger 显式体现 requested/effective 能力与资源留出。

## 目录

1. **[architecture.md](architecture.md)** — 组件划分、任务状态机、调度与 shim、持久化与恢复。
2. **[api.md](api.md)** — REST 路径、runtime capability/info/config/resources 接口、HTTP 状态码、请求与响应 JSON 字段说明。
3. **[cli.md](cli.md)** — `execgo-runtime` 各子命令与常用参数，包括 capability override 与容量覆盖选项。
4. **[deployment.md](deployment.md)** — 二进制部署、Docker 示例、健康检查、runtime 资源/能力抓取建议、CI/CD、版本与标签策略。
5. **[development.md](development.md)** — 本地构建、测试、代码风格与提交约定，以及 capability/policy/ledger 相关模块说明。

## 对外索引

- 仓库根目录 [README.md](../README.md) 提供项目概览与快速开始。
- 健康检查：`GET /healthz` 返回 `version` 字段，与 Cargo 包版本一致。
- runtime 自描述：`GET /api/v1/runtime/info`、`/api/v1/runtime/capabilities`、`/api/v1/runtime/config`、`/api/v1/runtime/resources`。
