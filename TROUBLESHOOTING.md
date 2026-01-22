# 故障排查文档

## 问题：在 musl 静态链接环境下 sleep/timer 卡住

### 问题描述

在 Docker scratch 镜像（musl 静态链接）环境下，使用 `tokio::time::sleep()` 会导致程序卡住，无法继续处理后续消息。

**现象：**
- 每次处理到第 6-9 篇文章时卡住
- 日志停在 `[DELAY-X] 开始等待 1000ms`，永远没有 `等待完成`
- 即使添加超时保护（`timeout()`）也不会触发
- 移除延迟后，所有消息都能正常发送

### 根本原因分析

#### 1. 技术背景

当前运行环境：
```
Docker 镜像: scratch (完全空的镜像)
C 标准库: musl (轻量级 libc)
链接方式: 静态链接
TLS 库: rustls (纯 Rust 实现)
HTTP 客户端: reqwest (异步)
异步运行时: tokio
```

#### 2. 为什么 tokio::time::sleep() 卡住？

tokio 的 timer 实现依赖系统调用：

**Linux 上：**
```rust
tokio::time::sleep()
    ↓
timerfd_create()  // 创建 timer 文件描述符
    ↓
epoll_ctl()       // 注册到 epoll
    ↓
epoll_wait()     // 等待 timer 事件 ❌ 卡在这里，永远不会唤醒
```

**在 musl 静态环境中的问题：**
- musl 的 `timerfd` 包装可能有 bug
- 静态链接可能缺失必要的运行时库
- epoll 在 scratch 环境下无法正确触发 timer 事件
- 整个异步任务被挂起，无法被调度器唤醒

**证据：**
- 添加 5 秒超时保护也没有触发
- `timeout(Duration::from_secs(5), sleep(...)).await` 卡住
- 说明不是 sleep 慢，而是任务无法被调度

#### 3. 为什么 spawn_blocking + thread::sleep() 也卡住？

尝试过使用阻塞线程池：

```rust
tokio::task::spawn_blocking(move || {
    std::thread::sleep(Duration::from_millis(100));
}).await.ok();
```

**结果：** 依然卡住，`await` 永远不会返回

**可能原因：**
- tokio 的 blocking 线程池管理器也依赖 timer
- 线程池在等待任务完成时，内部使用了有问题的 timer 机制
- 或者在 musl 环境下，线程池的调度逻辑有 bug

#### 4. 为什么 HTTP 请求正常？

```rust
reqwest → rustls → TCP socket → read/write ✅ 正常工作
```

**原因：**
- 网络操作不依赖 timer
- 直接使用系统调用（socket, connect, send, recv）
- rustls 是纯 Rust 实现，不依赖 OpenSSL
- 异步 IO 事件（可读/可写）由 epoll 正常触发

### 环境兼容性对比

| 组件 | 版本/实现 | musl 静态环境 | 问题 |
|------|-----------|---------------|------|
| **基础镜像** | scratch | ❌ | 极简环境，缺少系统库 |
| **C 标准库** | musl | ⚠️ | 对某些系统调用支持不完善 |
| **链接方式** | 静态链接 | ⚠️ | 可能导致运行时依赖缺失 |
| **TLS** | rustls | ✅ | 纯 Rust，无问题 |
| **HTTP** | reqwest (异步) | ✅ | 正常工作 |
| **HTTP** | ureq (同步) | ✅ | 正常工作 |
| **Timer** | tokio::time::sleep | ❌ | timerfd + epoll 卡住 |
| **Timer** | spawn_blocking + thread::sleep | ❌ | 线程池管理器卡住 |

### 解决方案

#### 方案 1：移除延迟 + 立即重试（当前方案）✅

**实现：**
```rust
match send_to_lark(&webhook_url, &payload).await {
    Ok(true) => {
        // 发送成功，立即处理下一篇
        success_count += 1;
        break;
    }
    Ok(false) => {
        // 429 限流，立即重试（不延迟）
        retries += 1;
        if retries >= MAX_RETRIES {
            error!("超过最大重试次数");
            break;
        }
        continue;  // 立即重试
    }
    Err(e) => {
        // 其他错误，跳过
        error!("请求失败: {}", e);
        break;
    }
}
```

**优点：**
- 不依赖任何 timer 机制
- 简单可靠
- 实测飞书限流宽松，10 条连续发送无问题

**缺点：**
- 理论上可能触发 429 限流（实际未遇到）
- 如果遇到 429，会立即重试（可能加重限流）

**测试结果：**
- ✅ 10 篇文章连续发送，总耗时约 5 秒
- ✅ 无 429 限流错误
- ✅ 在 musl 静态环境下稳定运行

#### 方案 2：使用 glibc 基础镜像

修改 Dockerfile 使用 alpine 或 debian 镜像：

```dockerfile
FROM alpine:latest
RUN apk add --no-cache ca-certificates
COPY --from=builder /app /app
CMD ["/app/rust-miniflux2feishu"]
```

**优点：**
- 完整的 glibc 环境
- tokio timer 正常工作
- 可以使用 `tokio::time::sleep()`

**缺点：**
- 镜像变大（alpine ~5MB，debian ~50MB vs scratch ~2MB）
- 失去 scratch 镜像的安全优势

#### 方案 3：在调用方实现延迟

让 Miniflux 或调用方控制发送频率：

```rust
// 服务端立即返回，不等待
StatusCode::OK

// Miniflux 配置：
// - 设置 Webhook 超时
// - 控制批量推送频率
```

**优点：**
- 服务端完全无延迟
- 由调用方控制节奏

**缺点：**
- 需要修改 Miniflux 配置
- 失去服务端的速率控制能力

### 经验教训

#### 1. musl 静态链接环境的限制

在 **scratch + musl** 环境下：
- ✅ 可以用：纯 Rust 实现的库（rustls, reqwest）
- ✅ 可以用：同步系统调用（socket, read, write）
- ❌ 不可用：依赖复杂运行时的功能（tokio timer）
- ❌ 不可用：需要特定系统库的功能

#### 2. 调试技巧

当遇到异步任务卡住时：
```rust
// 添加详细日志
eprintln!("[BEFORE] 进入 sleep");
sleep(Duration::from_millis(100)).await;
eprintln!("[AFTER] sleep 完成");  // 如果没打印，说明 sleep 卡住

// 检查是否是 timer 问题
// 尝试移除所有 sleep/timer/timeout
```

#### 3. 选择技术栈的考虑

**对于极简 Docker 镜像：**
- 优先选择纯 Rust 实现的库
- 避免依赖复杂的异步特性（timer, interval 等）
- 测试在实际运行环境中的表现，不只是本地

### 相关资源

- [tokio::time::sleep - 为什么会卡住？](https://github.com/tokio-rs/tokio/issues)
- [musl 静态链接的限制](https://wiki.musl-libc.org/doc/1.2.0/faq.html)
- [Docker scratch 镜像最佳实践](https://docs.docker.com/develop/develop-images/dockerfile_best-practices/#use-multi-stage-builds)

### 更新日志

- **2025-01-22**: 发现并修复 musl 环境下 sleep 卡住的问题
- **2025-01-22**: 从 ureq (同步) 迁移到 reqwest (异步) + rustls
- **2025-01-22**: 移除所有延迟机制，改为立即发送 + 429 重试
