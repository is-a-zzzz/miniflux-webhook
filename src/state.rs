use reqwest::Client;

// 使用 Arc 来安全地在多个异步任务中共享配置
pub struct AppState {
    pub lark_webhook_url: String,
    pub http_client: Client,
}
