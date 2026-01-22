# rust-miniflux2feishu

将 Miniflux RSS 订阅更新实时推送到飞书机器人的转发服务。

## 功能特性

- 实时接收 Miniflux webhook 通知
- 推送到飞书群机器人
- 自动生成 Miniflux 文章链接
- 429 限流自动重试（指数退避）
- 轻量级 Docker 镜像 (~2MB)
- Rust 实现，高性能稳定

## 快速开始

### 方式一：Docker 部署（推荐）

```bash
# 1. 克隆仓库
git clone https://github.com/is-a-zzzz/rust-miniflux2feishu.git
cd rust-miniflux2feishu

# 2. 复制配置模板
cp .env.example .env

# 3. 编辑配置，设置飞书 Webhook URL
vim .env
```

`.env` 文件内容：
```bash
# 必填：飞书机器人 Webhook URL
WEBHOOK_URL=https://open.feishu.cn/open-apis/bot/v2/hook/your-webhook-url

# 可选：Miniflux 服务器地址（用于生成文章链接）
MINIFLUX_URL=https://miniflux.example.com

# 可选：其他配置
RUST_LOG=info
IP=0.0.0.0
PORT=8083
```

```bash
# 4. 构建镜像（带日期标签）
./build.sh

# 或使用 docker compose
docker compose build

# 5. 启动服务
docker compose up -d

# 6. 查看日志
docker compose logs -f
```

### 方式二：本地运行

```bash
# 编译
cargo build --release

# 启动
./target/release/rust-miniflux2feishu \
  -w https://open.feishu.cn/open-apis/bot/v2/hook/your-webhook-url \
  -m https://miniflux.example.com
```

## 配置说明

| 参数 | 环境变量 | 必填 | 默认值 | 说明 |
|------|----------|------|--------|------|
| `-w, --webhook-url` | `WEBHOOK_URL` | 是 | - | 飞书机器人 Webhook URL |
| `-m, --miniflux-url` | `MINIFLUX_URL` | 否 | 空 | Miniflux 服务器地址 |
| `-i, --ip` | `IP` | 否 | 0.0.0.0 | 监听地址 |
| `-p, --port` | `PORT` | 否 | 8083 | 监听端口 |
| - | `RUST_LOG` | 否 | info | 日志级别 |

### 获取飞书 Webhook URL

1. 打开飞书群
2. 群设置 → 群机器人 → 添加机器人 → 自定义机器人
3. 复制 Webhook URL

## Miniflux 配置

在 Miniflux 中设置 Webhook：

```
https://your-server.com:8083/webhook
```

## 飞书推送格式

每篇文章会单独推送一张卡片：

```
┌─────────────────────────────┐
│ 【文章标题】                 │ ← 加粗显示
│                             │
│ Miniflux                    │
│ 原文                        │
└─────────────────────────────┘
```

**Miniflux** 链接格式：
```
https://miniflux.example.com/rss/feed/{feed_id}/entry/{entry_id}
```

## 测试

使用提供的测试数据：

```bash
curl -X POST http://127.0.0.1:8083/webhook \
  -H "Content-Type: application/json" \
  -d @test_payload.json
```

## 限流处理

服务自动处理飞书 429 限流：

- 最多重试 3 次
- 遇到 429 立即重试（不延迟，实测飞书限流较宽松）
- Webhook 串行处理（全局锁，防止并发冲突）

**注意**：在 musl 静态链接环境下，为了避免 timer 卡住问题，采用立即发送策略。实际测试表明，连续发送 10 条消息不会触发 429 限流。

## Docker 镜像

- **基础镜像**: scratch
- **架构**: linux/amd64
- **大小**: 约 2-3 MB

使用 `build.sh` 构建的镜像标签：
- `rust-miniflux2feishu:YYYYMMDD` （日期标签）
- `rust-miniflux2feishu:latest`

## 生产环境建议

### 1. 使用反向代理

```nginx
location /webhook {
    proxy_pass http://127.0.0.1:8083;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
}
```

### 2. 日志限制

在 `docker-compose.yml` 中添加：

```yaml
logging:
  driver: "json-file"
  options:
    max-size: "10m"
    max-file: "3"
```

### 3. 自动重启

```yaml
restart: unless-stopped
```

## 开发

```bash
# 运行
cargo run

# 测试
cargo test

# 构建
cargo build --release
```

## 技术实现

### HTTP 客户端选择

使用 **reqwest**（异步）+ **rustls**（纯 Rust TLS）：

1. **异步高性能**：适合处理大量并发 webhook 请求
2. **rustls**：避免 OpenSSL 依赖，适配 Docker musl 环境
3. **无延迟策略**：连续发送 + 429 重试，避免 timer 卡住问题

### 已知问题

在 **musl 静态链接** 环境下，tokio 的 timer 功能不可靠：
- `tokio::time::sleep()` 会卡住
- `tokio::time::timeout()` 也会失效
- `spawn_blocking + thread::sleep()` 同样卡住

**解决方案**：移除所有延迟机制，改为立即发送 + 重试策略。

详见：[TROUBLESHOOTING.md](TROUBLESHOOTING.md)

### 构建优化

Docker 多阶段构建 + 缓存层优化：

```dockerfile
# 1. 先复制 Cargo.toml，构建依赖（可缓存）
COPY Cargo.toml Cargo.lock ./
RUN cargo install --path . --root /

# 2. 再复制源码，只重编译自己的代码
COPY src ./src
RUN touch src/main.rs && cargo install --path . --root /
```

**优势**：依赖变更时才重新编译，源码改动只需 ~50 秒。

### 相关问题

详见故障排查文档：[TROUBLESHOOTING.md](TROUBLESHOOTING.md)

## License

MIT
