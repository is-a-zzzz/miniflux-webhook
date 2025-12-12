use axum::{
    Router,
    extract::{Json, State},
    http::StatusCode,
    routing::post,
};

use clap::Parser;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info}; // 引入 clap

// --- 0. 命令行参数结构体 ---

/// Miniflux Webhook 转发到飞书机器人的服务
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 监听的 IP 地址
    #[arg(short = 'i', long, default_value = "0.0.0.0")]
    ip: String,

    /// 监听的端口
    #[arg(short = 'p', long, default_value_t = 8081)]
    port: u16,

    /// 飞书机器人的 Webhook URL
    #[arg(short = 'w', long)]
    webhook_url: String,
}

// --- 1. 配置和状态 ---

// 使用 Arc 来安全地在多个异步任务中共享配置
struct AppState {
    lark_webhook_url: String,
    http_client: Client,
}

// --- 2. Miniflux 数据结构定义 ---

// 定义 Miniflux Webhook 发送的 JSON 结构
#[derive(Debug, Deserialize)]
struct MinifluxEntry {
    title: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct MinifluxWebhook {
    feed_title: String,
    entries: Vec<MinifluxEntry>,
}

// --- 3. 飞书富文本消息结构定义 ---

// 飞书消息的顶层结构
#[derive(Debug, Serialize)]
struct LarkMessage {
    msg_type: &'static str,
    content: LarkContent,
}

#[derive(Debug, Serialize)]
struct LarkContent {
    post: LarkPost,
}

#[derive(Debug, Serialize)]
struct LarkPost {
    zh_cn: LarkLanguageContent,
}

#[derive(Debug, Serialize)]
struct LarkLanguageContent {
    title: String,
    content: Vec<Vec<LarkElement>>,
}

// 飞书支持的元素类型
#[derive(Debug, Serialize)]
#[serde(tag = "tag", rename_all = "snake_case")]
enum LarkElement {
    Text { text: String },
    A { text: String, href: String },
    At { user_id: String },
}

// --- 4. 构造飞书消息函数 ---

fn build_lark_payload(entry: &MinifluxEntry, feed_title: &str) -> LarkMessage {
    LarkMessage {
        msg_type: "post",
        content: LarkContent {
            post: LarkPost {
                zh_cn: LarkLanguageContent {
                    title: format!("Miniflux 更新: {}", feed_title),
                    content: vec![
                        // 第一段：@ 所有人
                        vec![
                            LarkElement::Text {
                                text: "有新的订阅文章到达，请查收！".to_string(),
                            },
                            LarkElement::At {
                                user_id: "all".to_string(), // @ 所有人
                            },
                        ],
                        // 第二段：文章链接
                        vec![
                            LarkElement::Text {
                                text: "文章标题: ".to_string(),
                            },
                            LarkElement::A {
                                text: entry.title.clone(),
                                href: entry.url.clone(),
                            },
                        ],
                    ],
                },
            },
        },
    }
}

// --- 5. Webhook 处理函数 ---

async fn handle_miniflux_webhook(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<MinifluxWebhook>,
) -> StatusCode {
    if payload.entries.is_empty() {
        return StatusCode::OK; // 没有新文章，正常返回
    }

    let entry = &payload.entries[0]; // 仅处理第一篇新文章

    info!(
        "接收到 Miniflux 更新：{} - {}",
        payload.feed_title, entry.title
    );

    // 构造飞书消息体
    let lark_payload = build_lark_payload(entry, &payload.feed_title);

    // 发送 POST 请求到飞书 Webhook
    match state
        .http_client
        .post(&state.lark_webhook_url)
        .json(&lark_payload)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            info!("成功发送到飞书 Webhook");
            StatusCode::OK
        }
        Ok(response) => {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "无法读取响应体".to_string());
            error!("飞书 API 错误：状态码 {}，响应：{}", status, text);
            StatusCode::INTERNAL_SERVER_ERROR
        }
        Err(e) => {
            error!("发送请求失败: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

// --- 6. 主函数 ---

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 解析命令行参数
    let args = Args::parse();

    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!(
        "服务配置：IP={}, Port={}, Webhook={}",
        args.ip, args.port, args.webhook_url
    );

    // 检查 Webhook URL 是否已提供 (因为我们在 Args 中没有设置默认值，所以这里只是强调)
    if args.webhook_url.is_empty() || args.webhook_url == "" {
        error!("请通过 -w 或 --webhook-url 参数提供飞书 Webhook URL。");
        return Err("缺少 Webhook URL 参数".into());
    }

    let app_state = Arc::new(AppState {
        lark_webhook_url: args.webhook_url,
        http_client: Client::builder() // 使用 builder 方法
            .use_rustls_tls() // 明确指定使用 rustls
            .build()
            .expect("无法创建 reqwest 客户端"),
    });

    // 拼接监听地址
    let addr = format!("{}:{}", args.ip, args.port);

    // 定义 Webhook 路由
    let app = Router::new()
        .route("/webhook", post(handle_miniflux_webhook))
        .with_state(app_state);

    info!("服务正在监听：{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
