use clap::Parser;

/// Miniflux Webhook 转发到飞书机器人的服务
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// 监听的 IP 地址
    #[arg(short = 'i', long, default_value = "0.0.0.0")]
    pub ip: String,

    /// 监听的端口
    #[arg(short = 'p', long, default_value_t = 8081)]
    pub port: u16,

    /// 飞书机器人的 Webhook URL
    #[arg(short = 'w', long)]
    pub webhook_url: String,
}
