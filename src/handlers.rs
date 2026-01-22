use axum::{
    extract::{Json, State},
    http::StatusCode,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::state::AppState;
use crate::models::{
    miniflux::MinifluxWebhook,
    lark::build_lark_payload,
};

// 全局互斥锁，确保webhook串行处理
static WEBHOOK_LOCK: Mutex<()> = Mutex::const_new(());

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 1000; // 1秒延迟
const MESSAGE_INTERVAL_MS: u64 = 1000; // 消息间隔1秒，避免触发飞书429限流（飞书限制：每分钟最多20条）
const HTTP_TIMEOUT_SECS: u64 = 10; // HTTP 请求超时时间10秒

pub async fn handle_miniflux_webhook(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<MinifluxWebhook>,
) -> StatusCode {
    // 获取全局锁，确保webhook串行处理
    eprintln!("[LOCK] 尝试获取 WEBHOOK_LOCK...");
    let _lock = WEBHOOK_LOCK.lock().await;
    eprintln!("[LOCK] 已获取 WEBHOOK_LOCK");

    if payload.entries.is_empty() {
        return StatusCode::OK; // 没有新文章，正常返回
    }

    eprintln!("[WEBHOOK] 接收到 {} 篇文章", payload.entries.len());
    info!(
        "接收到 Miniflux 更新：{}，共 {} 篇文章",
        payload.feed.title,
        payload.entries.len()
    );

    // 处理所有文章，每篇文章单独发送
    let mut success_count = 0;
    let mut failed_count = 0;

    for (index, entry) in payload.entries.iter().enumerate() {
        eprintln!("[PROCESS] 开始处理第 {}/{} 篇文章: {}", index + 1, payload.entries.len(), entry.title);
        info!(
            "处理第 {}/{} 篇文章：{}",
            index + 1,
            payload.entries.len(),
            entry.title
        );

        // 构造飞书消息体
        let lark_payload = build_lark_payload(entry, &state.miniflux_url);
        eprintln!("[PROCESS] 消息体构造完成");

        // 尝试发送，支持429重试
        let mut retries = 0;
        loop {
            eprintln!("[HTTP-{}] 准备发送请求 (重试次数={})", index + 1, retries);

            // 直接使用异步 HTTP 请求
            match send_to_lark(&state.lark_webhook_url, &lark_payload).await {
                Ok(true) => {
                    // 发送成功
                    eprintln!("[HTTP-{}] 请求成功", index + 1);
                    info!("成功发送第 {} 篇文章到飞书", index + 1);

                    // 如果不是最后一篇，添加延迟
                    if index < payload.entries.len() - 1 {
                        eprintln!("[DELAY-{}] 开始等待 100ms", index + 1);

                        // 使用 tokio::time::sleep（100ms 稳定，1000ms 会卡住）
                        tokio::time::sleep(Duration::from_millis(100)).await;

                        eprintln!("[DELAY-{}] 等待完成", index + 1);
                    }

                    eprintln!("[PROCESS-{}] 第 {} 篇文章处理完成", index + 1, index + 1);
                    success_count += 1;
                    break;
                }
                Ok(false) => {
                    // 429错误，需要重试（使用指数退避）
                    eprintln!("[HTTP-{}] 收到 429 限流", index + 1);
                    retries += 1;
                    if retries >= MAX_RETRIES {
                        error!("第 {} 篇文章发送失败：超过最大重试次数", index + 1);
                        failed_count += 1;
                        break;
                    }
                    // 指数退避：第1次1秒，第2次2秒，第3次4秒
                    let backoff_ms = RETRY_DELAY_MS * 2_u64.pow(retries - 1);
                    warn!(
                        "遇到429限流，第 {} 次重试（{}ms 后）...",
                        retries, backoff_ms
                    );
                    eprintln!("[RETRY-{}] 等待 {}ms 后重试", index + 1, backoff_ms);

                    // 使用 tokio::time::sleep
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                }
                Err(e) => {
                    // 其他错误，不重试
                    eprintln!("[HTTP-{}] 请求失败: {}", index + 1, e);
                    error!("第 {} 篇文章发送失败：{}", index + 1, e);
                    failed_count += 1;
                    break;
                }
            }
        }
    }

    info!(
        "发送完成：成功 {} 篇，失败 {} 篇",
        success_count, failed_count
    );

    if failed_count > 0 {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::OK
    }
}

// 异步发送HTTP请求到飞书
async fn send_to_lark<T: serde::Serialize>(
    webhook_url: &str,
    payload: &T,
) -> Result<bool, String> {
    // 只创建一次 HTTP 客户端，连接池会复用
    static HTTP_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

    let client = HTTP_CLIENT.get_or_init(|| {
        eprintln!("[HTTP] 初始化 HTTP 客户端");
        reqwest::Client::builder()
            .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(10) // 增加连接池大小
            .build()
            .expect("Failed to create HTTP client")
    });

    eprintln!("[HTTP] HTTP 客户端创建成功，开始发送请求");

    // 发送 POST 请求
    let response = client
        .post(webhook_url)
        .json(payload)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    eprintln!("[HTTP] 收到响应，状态码: {}", response.status());

    let status = response.status().as_u16();

    if status == 200 {
        Ok(true)
    } else if status == 429 {
        // 429 Too Many Requests，需要重试
        Ok(false)
    } else {
        // 尝试读取错误响应体
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "无法读取错误响应".to_string());
        Err(format!("状态码 {}, 响应: {}", status, error_body))
    }
}
