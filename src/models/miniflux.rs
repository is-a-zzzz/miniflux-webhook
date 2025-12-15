use serde::Deserialize;

// 定义 Miniflux Webhook 发送的 JSON 结构
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct MinifluxEntry {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct MinifluxWebhook {
    pub feed_title: String,
    pub entries: Vec<MinifluxEntry>,
}
