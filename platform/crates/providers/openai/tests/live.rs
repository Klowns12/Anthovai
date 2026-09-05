//! The OpenAI provider against the real API.
//!
//! Everything else in this workspace is tested against stand-ins, which is
//! right — a test that calls a vendor measures the vendor. But three things
//! cannot be known any other way: whether the model names in
//! `config/models.toml` still exist, whether the request shape is still what
//! the API accepts, and whether the usage numbers we bill on come back at all.
//!
//! Ignored by default. Each run costs a fraction of a cent.
//!
//! ```text
//! OPENAI_API_KEY=… cargo test -p anthovai-provider-openai --test live -- --ignored --nocapture
//! ```

use anthovai_embeddings::EmbeddingProvider;
use anthovai_inference::types::{
    ChatMessage, ChatRequest, ChatRole, FinishReason, ProviderId, ReasoningLevel,
};
use anthovai_inference::ChatProvider;
use anthovai_provider_openai::{OpenAiEmbeddings, OpenAiProvider};

/// The models the registry names, so this test fails when one is retired.
///
/// Retirement is not hypothetical: OpenAI publishes shutdown dates, and a model
/// that disappears turns every question on that tier into a failover, or into
/// `provider_unavailable` when the tier has nothing else.
const REGISTRY_MODELS: &[&str] = &["gpt-5.4-nano", "gpt-5.4-mini", "gpt-5.5"];

const EMBEDDING_MODEL: &str = "text-embedding-3-small";
const DIMENSION: usize = 1536;

fn key() -> Option<String> {
    std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
}

fn request(model: &str, question: &str) -> ChatRequest {
    ChatRequest {
        model: model.to_owned(),
        system: "You answer in one short sentence, from the knowledge given to you.".to_owned(),
        messages: vec![ChatMessage {
            role: ChatRole::User,
            content: question.to_owned(),
        }],
        max_tokens: 2_000,
        reasoning: ReasoningLevel::Fast,
        stop: Vec::new(),
        // A hash in production. Any opaque string will do here — what matters
        // is that the field is accepted, because a rejected `user` field would
        // break every request.
        tenant_hash: "test-tenant-hash".to_owned(),
        request_id: "req_live_test".to_owned(),
    }
}

#[tokio::test]
#[ignore = "needs OPENAI_API_KEY and makes a real, billable API call"]
async fn every_model_in_the_registry_answers() {
    let Some(key) = key() else {
        println!("OPENAI_API_KEY is not set; nothing to check");
        return;
    };

    let provider = OpenAiProvider::new(key, None).expect("build the provider");
    assert_eq!(provider.id(), ProviderId::OpenAi);

    let mut failures = Vec::new();

    for model in REGISTRY_MODELS {
        let result = provider
            .chat(request(model, "What is the capital of Thailand?"))
            .await;

        match result {
            Ok(response) => {
                println!(
                    "{model:<16} {:>4} in / {:>4} out  finish={:?}  answered={:?}",
                    response.usage.input_tokens,
                    response.usage.output_tokens,
                    response.finish,
                    truncate(&response.text, 60),
                );

                if response.text.trim().is_empty() {
                    failures.push(format!("{model}: answered with nothing"));
                }
                // Usage is what a customer is billed on. A provider that
                // stopped reporting it would leave every invoice at zero, and
                // nothing else in the system would notice.
                if response.usage.input_tokens == 0 {
                    failures.push(format!("{model}: reported zero input tokens"));
                }
                if response.usage.output_tokens == 0 {
                    failures.push(format!("{model}: reported zero output tokens"));
                }
                // `Other` means a finish reason we do not recognise, which is
                // how a change in the API surfaces here first.
                if let FinishReason::Other(reason) = &response.finish {
                    failures.push(format!("{model}: unmapped finish reason `{reason}`"));
                }
                // The response says which model actually served the request.
                // An alias silently resolving elsewhere is worth knowing.
                if !response.model.starts_with(model) {
                    println!("  note: `{model}` was served by `{}`", response.model);
                }
            }
            Err(e) => failures.push(format!("{model}: {e}")),
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
#[ignore = "needs OPENAI_API_KEY and makes a real, billable API call"]
async fn a_length_limit_comes_back_as_a_length_finish() {
    // The one finish reason with a consequence: an answer cut off mid-sentence
    // should be recognisable as truncated rather than treated as complete.
    let Some(key) = key() else {
        return;
    };

    let provider = OpenAiProvider::new(key, None).expect("build the provider");
    let mut req = request(REGISTRY_MODELS[0], "Count slowly from one to two hundred.");
    req.max_tokens = 16;

    let response = provider.chat(req).await.expect("the call should succeed");
    println!("finish={:?} text={:?}", response.finish, response.text);

    assert!(
        matches!(response.finish, FinishReason::Length),
        "expected a length finish, got {:?}",
        response.finish
    );
}

#[tokio::test]
#[ignore = "needs OPENAI_API_KEY and makes a real, billable API call"]
async fn a_model_that_does_not_exist_is_a_bad_request_not_a_retry() {
    // This distinction decides whether the router moves to the next candidate
    // or retries the same one. A retired model name retried forever would burn
    // the whole request budget on a model that is never coming back.
    let Some(key) = key() else {
        return;
    };

    let provider = OpenAiProvider::new(key, None).expect("build the provider");
    let error = provider
        .chat(request("gpt-does-not-exist", "hello"))
        .await
        .expect_err("a nonexistent model should fail");

    println!("{error:?}");
    assert!(
        !error.is_retryable(),
        "a nonexistent model must not be retried: {error:?}"
    );
}

#[tokio::test]
#[ignore = "needs OPENAI_API_KEY and makes a real, billable API call"]
async fn a_bad_key_is_an_auth_error_and_is_never_retried() {
    let provider = OpenAiProvider::new("sk-not-a-real-key", None).expect("build the provider");
    let error = provider
        .chat(request(REGISTRY_MODELS[0], "hello"))
        .await
        .expect_err("a bad key should fail");

    println!("{error:?}");
    assert!(
        !error.is_retryable(),
        "a bad key will still be bad on the next attempt: {error:?}"
    );
    assert!(
        !error.counts_against_health(),
        "our own misconfiguration must not open the circuit on a healthy model"
    );
}

#[tokio::test]
#[ignore = "needs OPENAI_API_KEY and makes a real, billable API call"]
async fn embeddings_come_back_at_the_dimension_the_schema_expects() {
    // `document_chunks.embedding` is `vector(1536)`. A model returning a
    // different width would fail on insert — after the whole document had been
    // parsed, chunked and paid for.
    let Some(key) = key() else {
        return;
    };

    let embedder =
        OpenAiEmbeddings::new(key, None, EMBEDDING_MODEL, DIMENSION).expect("build the embedder");

    assert_eq!(embedder.model_id(), format!("openai:{EMBEDDING_MODEL}"));

    let vectors = embedder
        .embed_batch(&[
            "The library opens at seven in the morning.".to_owned(),
            "หลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์".to_owned(),
        ])
        .await
        .expect("embed");

    assert_eq!(vectors.len(), 2, "one vector per input, in order");
    for vector in &vectors {
        assert_eq!(vector.len(), DIMENSION);
        assert!(
            vector.iter().any(|v| *v != 0.0),
            "an all-zero vector means the text never reached the model"
        );
    }

    // Thai and English about different things should not land on top of each
    // other. This is the cheapest possible check that the text arrived intact.
    let similarity = cosine(&vectors[0], &vectors[1]);
    println!("cosine between the two inputs: {similarity:.3}");
    assert!(
        similarity < 0.9,
        "two unrelated sentences embedded almost identically ({similarity:.3}), \
         which usually means the input was mangled"
    );
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm(a) * norm(b))
}

fn truncate(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}
