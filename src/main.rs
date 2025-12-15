use axum::{routing::post, Router};
use clap::Parser;
use reqwest::Client;
use std::sync::Arc;
use tracing::{error, info};

mod cli;
mod handlers;
mod models;
mod state;

use cli::Args;
use handlers::handle_miniflux_webhook;
use state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 解析命令行参数
    let args = Args::parse();

    // 初始化日志系统
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    info!(
        "服务配置：IP={}, Port={}, Webhook={}",
        args.ip, args.port, args.webhook_url
    );

    // 检查 Webhook URL 是否已提供 (因为我们在 Args 中没有设置默认值，所以这里只是强调)
    if args.webhook_url.is_empty() {
        error!("请通过 -w 或 --webhook-url 参数提供飞书 Webhook URL。");
        return Err("缺少 Webhook URL 参数".into());
    }

    let app_state = Arc::new(AppState {
        lark_webhook_url: args.webhook_url,
        http_client: Client::builder()
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