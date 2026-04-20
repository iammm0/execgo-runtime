# 本地开发

## 前置条件

- 安装 **Rust**（`rustup` 推荐，`stable` 工具链）。
- macOS / Linux 开发体验最佳；Windows 未列为一级支持。

## 常用命令

```bash
# 格式化
cargo fmt

# 静态检查
cargo clippy --all-targets --all-features

# 测试（含集成测试 e2e）
cargo test

# 运行服务
cargo run -- serve --listen-addr 127.0.0.1:8080 --data-dir ./data
```

集成测试位于 `tests/e2e.rs`，会启动真实子进程并访问 HTTP 端口。

## 目录结构（简要）

```text
src/
  main.rs       # 二进制入口
  lib.rs        # 库入口与模块声明
  cli.rs        # 命令行
  server.rs     # HTTP 路由
  runtime.rs    # 核心运行时
  repo.rs       # SQLite
  types.rs      # 数据模型
  metrics.rs    # Prometheus 文本
  error.rs      # 错误与 HTTP 映射
tests/
  e2e.rs        # 端到端测试
```

## 代码风格

- 与现有代码保持一致：错误用 `AppError` / `AppResult`，异步边界用 `tokio`。
- 提交前建议执行 `cargo fmt` 与 `cargo clippy`，与 CI 保持一致。

## 贡献流程

1. Fork 仓库并创建分支。
2. 小步提交，说明清楚**动机与行为变化**。
3. 推送并发起 Pull Request；确保 CI 通过。

若需新增面向用户的文档，请同步更新 `docs/README.md` 索引与根 `README.md` 链接。
