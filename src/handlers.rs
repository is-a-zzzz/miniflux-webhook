use axum::{
    extract::{Json, State},
    http::StatusCode,
};
use std::sync::Arc;
use tracing::{error, info};

use crate::state::AppState;
use crate::models::{
    miniflux::MinifluxWebhook,
    lark::build_lark_payload,
};

pub async fn handle_miniflux_webhook(
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
