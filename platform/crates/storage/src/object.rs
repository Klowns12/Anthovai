//! The real object store.
//!
//! One implementation covers local disk, MinIO and S3 because `object_store`
//! already abstracts them. That matters for more than tidiness: a developer can
//! run the whole platform with only PostgreSQL on their machine, and the code
//! path they exercise is the same one production takes.

use anthovai_core::config::StorageSettings;
use anthovai_core::{DomainError, Result};
use async_trait::async_trait;
use object_store::path::Path;
use object_store::{ObjectStore as _, ObjectStoreExt as _, PutPayload};

use crate::ObjectStorage;

pub struct ObjectStoreStorage {
    inner: Box<dyn object_store::ObjectStore>,
    /// Named in errors, so a misconfigured bucket says which one.
    description: String,
}

impl ObjectStoreStorage {
    /// Build from configuration. Fails at startup rather than on the first
    /// upload: a bucket that does not exist should stop a deployment, not
    /// surface as a 500 an hour later.
    pub fn from_settings(settings: &StorageSettings) -> Result<Self> {
        match settings.provider.as_str() {
            "local" => Self::local(&settings.local_path),
            "s3" => Self::s3(settings),
            other => Err(DomainError::Internal(anyhow::anyhow!(
                "unknown storage provider `{other}`, expected `local` or `s3`"
            ))),
        }
    }

    pub fn local(path: &str) -> Result<Self> {
        std::fs::create_dir_all(path).map_err(|e| {
            DomainError::Internal(anyhow::anyhow!("could not create `{path}`: {e}"))
        })?;
        let store = object_store::local::LocalFileSystem::new_with_prefix(path).map_err(|e| {
            DomainError::Internal(anyhow::anyhow!("local storage at `{path}`: {e}"))
        })?;

        Ok(Self {
            inner: Box::new(store),
            description: format!("local:{path}"),
        })
    }

    fn s3(settings: &StorageSettings) -> Result<Self> {
        // `from_env` rather than `new`, so the conventional AWS variables work.
        // Every managed host hands credentials over that way — DigitalOcean
        // Spaces, S3, Tigris, and an IAM role with no variables at all — and
        // `new` reads none of them.
        //
        // The failure that this fixes gave no useful signal: with no
        // credentials the client falls back to the EC2 metadata service at
        // 169.254.169.254, which on any other host simply never answers. Every
        // call spent about three seconds going nowhere and readiness reported
        // "object storage is not answering", which is true and says nothing
        // about why.
        //
        // Our own settings are applied after, so an explicit key still wins.
        let mut builder = object_store::aws::AmazonS3Builder::from_env()
            .with_bucket_name(&settings.bucket)
            .with_region(&settings.region);

        if let Some(endpoint) = &settings.endpoint {
            // MinIO in development speaks plain HTTP on localhost. Allowing it
            // only when an endpoint is set keeps real S3 on TLS.
            builder = builder
                .with_endpoint(endpoint)
                .with_allow_http(endpoint.starts_with("http://"))
                .with_virtual_hosted_style_request(false);
        }
        if let Some(key) = &settings.access_key {
            builder = builder.with_access_key_id(key);
        }
        if let Some(secret) = &settings.secret_key {
            builder = builder.with_secret_access_key(secret);
        }

        let store = builder.build().map_err(|e| {
            DomainError::Internal(anyhow::anyhow!(
                "could not open bucket `{}`: {e}",
                settings.bucket
            ))
        })?;

        Ok(Self {
            inner: Box::new(store),
            description: format!("s3:{}", settings.bucket),
        })
    }

    fn path(&self, key: &str) -> Result<Path> {
        Path::parse(key).map_err(|e| {
            DomainError::Internal(anyhow::anyhow!("`{key}` is not a valid object key: {e}"))
        })
    }

    fn wrap(&self, err: object_store::Error, key: &str) -> DomainError {
        match err {
            object_store::Error::NotFound { .. } => DomainError::NotFound("object"),
            other => DomainError::Internal(anyhow::anyhow!(
                "{} failed for `{key}`: {other}",
                self.description
            )),
        }
    }
}

#[async_trait]
impl ObjectStorage for ObjectStoreStorage {
    async fn put(&self, key: &str, bytes: Vec<u8>, _content_type: &str) -> Result<()> {
        let path = self.path(key)?;
        self.inner
            .put(&path, PutPayload::from(bytes))
            .await
            .map_err(|e| self.wrap(e, key))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.path(key)?;
        let result = self.inner.get(&path).await.map_err(|e| self.wrap(e, key))?;
        let bytes = result.bytes().await.map_err(|e| self.wrap(e, key))?;
        Ok(bytes.to_vec())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.path(key)?;
        match self.inner.delete(&path).await {
            Ok(()) => Ok(()),
            // Deleting something that is already gone is the state we wanted.
            Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(e) => Err(self.wrap(e, key)),
        }
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let path = self.path(key)?;
        match self.inner.head(&path).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(self.wrap(e, key)),
        }
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<u64> {
        use futures::StreamExt;

        let path = self.path(prefix.trim_end_matches('/'))?;
        let mut listing = self.inner.list(Some(&path));
        let mut removed = 0;

        while let Some(item) = listing.next().await {
            let meta = item.map_err(|e| self.wrap(e, prefix))?;
            self.inner
                .delete(&meta.location)
                .await
                .map_err(|e| self.wrap(e, prefix))?;
            removed += 1;
        }
        Ok(removed)
    }
}

impl std::fmt::Debug for ObjectStoreStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectStoreStorage")
            .field("store", &self.description)
            .finish()
    }
}
