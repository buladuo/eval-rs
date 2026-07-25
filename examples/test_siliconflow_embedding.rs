//! 验证 SiliconFlow embedding 模型（BAAI/bge-m3）可通过 rig 0.40 调用。
//!
//! 运行：
//! ```sh
//! EVAL_PROVIDERS__SILICONFLOW_EMBEDDING__API_KEY=sk-... \
//!   cargo run --example test_siliconflow_embedding
//! ```
//!
//! 说明：rig 0.40 的 OpenAI client 实现了 `EmbeddingModel` trait，
//! 通过 `ClientBuilder::base_url` 指向 SiliconFlow 的 OpenAI 兼容端点即可。

use rig::client::EmbeddingsClient;
use rig::embeddings::EmbeddingModel;
use rig::providers::openai::Client;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let api_key = std::env::var("EVAL_PROVIDERS__SILICONFLOW_EMBEDDING__API_KEY")
        .expect("EVAL_PROVIDERS__SILICONFLOW_EMBEDDING__API_KEY 未设置");

    // rig 0.40: Client::builder() 不接受参数，链式 api_key / base_url / build。
    // SiliconFlow 提供 OpenAI 兼容的 /v1/embeddings 接口。
    // 注意：默认的 `Client` 是 Responses API 客户端，但 SiliconFlow 不支持
    // Responses API；不过 `EmbeddingsClient` trait 只走 `/embeddings` 端点，
    // 与 Responses/Completions 互不影响，因此可以复用同一个 client。
    let client = Client::builder()
        .api_key(api_key)
        .base_url("https://api.siliconflow.cn/v1")
        .build()?;

    let model = client.embedding_model("BAAI/bge-m3");

    let text = "eval-rs 是一个模块化的 LLM 评测服务。";
    let embedding = model.embed_text(text).await?;

    println!("model:        BAAI/bge-m3");
    println!("input:        {text}");
    println!("dimensions:   {}", embedding.vec.len());
    println!(
        "first 5 dims: {:?}",
        &embedding.vec[..5.min(embedding.vec.len())]
    );
    println!("OK: SiliconFlow embedding 调用成功");
    Ok(())
}
