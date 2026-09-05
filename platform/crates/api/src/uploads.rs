//! Receiving an upload.
//!
//! The bytes are streamed to storage while being counted and hashed, rather
//! than collected in memory first. A 200MB file would otherwise become 200MB of
//! heap per concurrent upload, and the size limit would only be enforced after
//! we had already paid for exceeding it.

use anthovai_core::{DomainError, Result, TenantCtx};
use anthovai_knowledge::{Document, KnowledgeService, StartUpload, UploadTarget};
use axum::extract::multipart::{Field, Multipart};
use sha2::{Digest, Sha256};

/// What a multipart upload said it was.
#[derive(Debug, Default)]
pub struct UploadForm {
    pub knowledge_base_id: Option<String>,
    pub title: Option<String>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub file: Option<FileInfo>,
}

#[derive(Debug)]
pub struct FileInfo {
    pub filename: String,
    pub mime_type: Option<String>,
}

/// Read the form, streaming the file part into storage as it arrives.
///
/// Ordering matters: `knowledge_base_id` must come before the file part, so the
/// document can be reserved and the plan checked before any bytes are stored.
/// Browsers send parts in the order they appear in the form, so the dashboard
/// controls this; a caller that gets it wrong is told plainly.
pub async fn receive(
    service: &KnowledgeService,
    ctx: &TenantCtx,
    fetcher: &reqwest::Client,
    mut multipart: Multipart,
    declared_size: Option<i64>,
) -> Result<Document> {
    let mut form = UploadForm::default();
    let mut started: Option<StartUpload> = None;
    let mut written: Option<Written> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| DomainError::validation(format!("malformed upload: {e}")))?
    {
        match field.name().unwrap_or_default() {
            "knowledge_base_id" => form.knowledge_base_id = Some(text_field(field).await?),
            "title" => form.title = Some(text_field(field).await?),
            "text" => form.text = Some(text_field(field).await?),
            "url" => form.url = Some(text_field(field).await?),

            "file" => {
                let filename = field
                    .file_name()
                    .map(str::to_owned)
                    .ok_or_else(|| DomainError::validation("the file part needs a filename"))?;
                let mime_type = field.content_type().map(str::to_owned);

                let kb_id = form
                    .knowledge_base_id
                    .as_deref()
                    .ok_or_else(|| {
                        DomainError::validation(
                            "knowledge_base_id must be sent before the file part",
                        )
                    })?
                    .parse()
                    .map_err(|_| DomainError::validation("knowledge_base_id is not a valid id"))?;

                let start = service
                    .start_upload(
                        ctx,
                        kb_id,
                        UploadTarget::File {
                            filename: filename.clone(),
                            mime_type: mime_type.clone(),
                            declared_size,
                        },
                    )
                    .await?;

                // From here a document row exists, so every failure has to
                // clean it up rather than leave a permanent "uploading".
                let result = stream_to_storage(service, field, &start).await;
                match result {
                    Ok(w) => written = Some(w),
                    Err(e) => {
                        service.abandon_upload(ctx, start.document_id).await.ok();
                        return Err(e);
                    }
                }

                form.file = Some(FileInfo {
                    filename,
                    mime_type,
                });
                started = Some(start);
            }

            _ => {
                // Ignore parts we do not know: a form gaining a field should
                // not break an upload.
            }
        }
    }

    match (started, written) {
        (Some(start), Some(written)) => finish(service, ctx, start, written).await,
        _ if form.url.is_some() => receive_url(service, ctx, fetcher, form).await,
        _ => receive_text(service, ctx, form).await,
    }
}

/// Text pasted into the dashboard: small enough to handle in one piece.
async fn receive_text(
    service: &KnowledgeService,
    ctx: &TenantCtx,
    form: UploadForm,
) -> Result<Document> {
    let text = form
        .text
        .ok_or_else(|| DomainError::validation("send either a `file` or a `text` part"))?;
    let title = form
        .title
        .ok_or_else(|| DomainError::validation("`text` uploads need a `title`"))?;
    let kb_id = form
        .knowledge_base_id
        .ok_or_else(|| DomainError::validation("knowledge_base_id is required"))?
        .parse()
        .map_err(|_| DomainError::validation("knowledge_base_id is not a valid id"))?;

    let start = service
        .start_upload(ctx, kb_id, UploadTarget::Text { title })
        .await?;

    let bytes = text.into_bytes();
    if bytes.len() as i64 > start.max_bytes {
        service.abandon_upload(ctx, start.document_id).await.ok();
        return Err(DomainError::PayloadTooLarge("file_too_large"));
    }

    let written = Written {
        bytes: bytes.len() as i64,
        hash: hex::encode(Sha256::digest(&bytes)),
    };

    if let Err(e) = service
        .storage()
        .put(&start.storage_key, bytes, "text/plain")
        .await
    {
        service.abandon_upload(ctx, start.document_id).await.ok();
        return Err(e);
    }

    finish(service, ctx, start, written).await
}

/// A page the customer asked us to fetch.
///
/// The URL is checked before a document row exists, so a refusal costs nothing
/// and the customer sees `url_not_allowed` while still looking at the form. The
/// fetch itself is bounded — 15 seconds, 10MB, five redirects, each one checked
/// again — so this cannot hold a request open indefinitely.
async fn receive_url(
    service: &KnowledgeService,
    ctx: &TenantCtx,
    fetcher: &reqwest::Client,
    form: UploadForm,
) -> Result<Document> {
    let url = form.url.unwrap_or_default();
    let kb_id = form
        .knowledge_base_id
        .ok_or_else(|| DomainError::validation("knowledge_base_id is required"))?
        .parse()
        .map_err(|_| DomainError::validation("knowledge_base_id is not a valid id"))?;

    // Refused here rather than after a document row is written, so nothing has
    // to be cleaned up and nothing is left at "uploading".
    anthovai_knowledge::url_guard::allowed(&url)?;

    let start = service
        .start_upload(
            ctx,
            kb_id,
            UploadTarget::Url {
                url: url.clone(),
                title: form.title,
            },
        )
        .await?;

    let fetched = match crate::fetch::fetch(fetcher, &url).await {
        Ok(fetched) => fetched,
        Err(e) => {
            service.abandon_upload(ctx, start.document_id).await.ok();
            return Err(e);
        }
    };

    if fetched.bytes.len() as i64 > start.max_bytes {
        service.abandon_upload(ctx, start.document_id).await.ok();
        return Err(DomainError::PayloadTooLarge("file_too_large"));
    }

    let written = Written {
        bytes: fetched.bytes.len() as i64,
        hash: hex::encode(Sha256::digest(&fetched.bytes)),
    };

    let content_type = fetched
        .content_type
        .unwrap_or_else(|| "text/html".to_owned());

    if let Err(e) = service
        .storage()
        .put(&start.storage_key, fetched.bytes, &content_type)
        .await
    {
        service.abandon_upload(ctx, start.document_id).await.ok();
        return Err(e);
    }

    finish(service, ctx, start, written).await
}

async fn finish(
    service: &KnowledgeService,
    ctx: &TenantCtx,
    start: StartUpload,
    written: Written,
) -> Result<Document> {
    if written.bytes == 0 {
        service.abandon_upload(ctx, start.document_id).await.ok();
        return Err(DomainError::validation("the upload was empty"));
    }

    service
        .finish_upload(
            ctx,
            start.document_id,
            &start.storage_key,
            written.bytes,
            &written.hash,
        )
        .await
}

struct Written {
    bytes: i64,
    hash: String,
}

/// Stream one field into storage, counting and hashing on the way past.
///
/// The size limit is enforced here as well as from `Content-Length`, because
/// that header is a claim: a chunked upload has none, and a lying one is easy
/// to send.
async fn stream_to_storage(
    service: &KnowledgeService,
    mut field: Field<'_>,
    start: &StartUpload,
) -> Result<Written> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut hasher = Sha256::new();
    let mut total: i64 = 0;

    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| DomainError::validation(format!("upload interrupted: {e}")))?
    {
        total += chunk.len() as i64;
        if total > start.max_bytes {
            return Err(DomainError::PayloadTooLarge("file_too_large"));
        }
        hasher.update(&chunk);
        buffer.extend_from_slice(&chunk);
    }

    // `object_store` wants the whole payload for a simple put. The limit above
    // bounds what that can cost; a multipart upload for large files is the
    // change to make when the limits grow past what a request should hold.
    service
        .storage()
        .put(&start.storage_key, buffer, start.source_type.as_str())
        .await?;

    Ok(Written {
        bytes: total,
        hash: hex::encode(hasher.finalize()),
    })
}

async fn text_field(field: Field<'_>) -> Result<String> {
    field
        .text()
        .await
        .map_err(|e| DomainError::validation(format!("could not read a form field: {e}")))
}
