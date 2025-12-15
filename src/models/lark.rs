use serde::Serialize;
use crate::models::miniflux::MinifluxEntry;

// 飞书消息的顶层结构
#[derive(Debug, Serialize)]
pub struct LarkMessage {
    pub msg_type: &'static str,
    pub content: LarkContent,
}

#[derive(Debug, Serialize)]
pub struct LarkContent {
    pub post: LarkPost,
}

#[derive(Debug, Serialize)]
pub struct LarkPost {
    pub zh_cn: LarkLanguageContent,
}

#[derive(Debug, Serialize)]
pub struct LarkLanguageContent {
    pub title: String,
    pub content: Vec<Vec<LarkElement>>,
}

// 飞书支持的元素类型
#[derive(Debug, Serialize)]
#[serde(tag = "tag", rename_all = "snake_case")]
pub enum LarkElement {
    Text { text: String },
    A { text: String, href: String },
    At { user_id: String },
}

// --- 4. 构造飞书消息函数 ---

pub fn build_lark_payload(entry: &MinifluxEntry, feed_title: &str) -> LarkMessage {
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
