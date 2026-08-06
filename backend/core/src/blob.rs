use async_trait::async_trait;
use bytes::Bytes;
use chrono::NaiveDate;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, BlobError>;

#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("blob not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("encryption error: {0}")]
    Encryption(String),
}

#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, key: &str, data: Bytes) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Bytes>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn exists(&self, key: &str) -> Result<bool>;
}

// ── Key helpers ──────────────────────────────────────────────────────────────

pub fn blob_key_body(account_id: &str, uid: u32, internal_date: NaiveDate) -> String {
    format!(
        "mail/{}/{}/{}/{}/{}/body",
        account_id,
        internal_date.format("%Y"),
        internal_date.format("%m"),
        internal_date.format("%d"),
        uid,
    )
}

pub fn blob_key_attachment(
    account_id: &str,
    uid: u32,
    internal_date: NaiveDate,
    n: usize,
) -> String {
    format!(
        "mail/{}/{}/{}/{}/{}/attach/{}",
        account_id,
        internal_date.format("%Y"),
        internal_date.format("%m"),
        internal_date.format("%d"),
        uid,
        n,
    )
}

// ── Local filesystem backend ─────────────────────────────────────────────────

pub struct LocalBlobStore {
    root: std::path::PathBuf,
}

impl LocalBlobStore {
    pub fn new(root: impl Into<std::path::PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[async_trait]
impl BlobStore for LocalBlobStore {
    async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        let path = self.root.join(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, data).await?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Bytes> {
        let path = self.root.join(key);
        match tokio::fs::read(&path).await {
            Ok(data) => Ok(Bytes::from(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(BlobError::NotFound(key.to_string()))
            }
            Err(e) => Err(BlobError::Io(e)),
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.root.join(key);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(BlobError::Io(e)),
        }
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let path = self.root.join(key);
        Ok(tokio::fs::try_exists(&path).await?)
    }
}

// ── S3-compatible backend ─────────────────────────────────────────────────────

pub struct S3BlobStore {
    store: object_store::aws::AmazonS3,
    _bucket: String,
}

impl S3BlobStore {
    pub fn from_env() -> std::result::Result<Self, String> {
        let bucket = std::env::var("S3_BUCKET")
            .map_err(|_| "S3_BUCKET env var required when BLOB_BACKEND=s3".to_string())?;
        let region = std::env::var("S3_REGION")
            .map_err(|_| "S3_REGION env var required when BLOB_BACKEND=s3".to_string())?;
        let access_key = std::env::var("AWS_ACCESS_KEY_ID")
            .map_err(|_| "AWS_ACCESS_KEY_ID env var required when BLOB_BACKEND=s3".to_string())?;
        let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY").map_err(|_| {
            "AWS_SECRET_ACCESS_KEY env var required when BLOB_BACKEND=s3".to_string()
        })?;

        let mut builder = object_store::aws::AmazonS3Builder::new()
            .with_bucket_name(&bucket)
            .with_region(&region)
            .with_access_key_id(&access_key)
            .with_secret_access_key(&secret_key);

        if let Ok(endpoint) = std::env::var("S3_ENDPOINT") {
            builder = builder.with_endpoint(&endpoint);
        }

        let store = builder
            .build()
            .map_err(|e| format!("failed to build S3 store: {e}"))?;

        Ok(Self {
            store,
            _bucket: bucket,
        })
    }
}

#[async_trait]
impl BlobStore for S3BlobStore {
    async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        use object_store::ObjectStore;
        let path = object_store::path::Path::from(key);
        self.store
            .put(&path, data.into())
            .await
            .map_err(|e| BlobError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Bytes> {
        use object_store::ObjectStore;
        let path = object_store::path::Path::from(key);
        match self.store.get(&path).await {
            Ok(result) => {
                let bytes = result
                    .bytes()
                    .await
                    .map_err(|e| BlobError::Storage(e.to_string()))?;
                Ok(bytes)
            }
            Err(object_store::Error::NotFound { .. }) => Err(BlobError::NotFound(key.to_string())),
            Err(e) => Err(BlobError::Storage(e.to_string())),
        }
    }

    async fn delete(&self, key: &str) -> Result<()> {
        use object_store::ObjectStore;
        let path = object_store::path::Path::from(key);
        match self.store.delete(&path).await {
            Ok(()) => Ok(()),
            Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(e) => Err(BlobError::Storage(e.to_string())),
        }
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        use object_store::ObjectStore;
        let path = object_store::path::Path::from(key);
        match self.store.head(&path).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(BlobError::Storage(e.to_string())),
        }
    }
}

// ── Encrypting wrapper ────────────────────────────────────────────────────────

pub struct EncryptingBlobStore {
    inner: Arc<dyn BlobStore>,
    key: aes_gcm::Key<aes_gcm::Aes256Gcm>,
}

impl EncryptingBlobStore {
    pub fn new(inner: Arc<dyn BlobStore>, key_bytes: &[u8; 32]) -> Self {
        use aes_gcm::Key;
        Self {
            inner,
            key: Key::<aes_gcm::Aes256Gcm>::from(*key_bytes),
        }
    }
}

#[async_trait]
impl BlobStore for EncryptingBlobStore {
    async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        use aes_gcm::{AeadInPlace, KeyInit, Nonce};
        use rand::RngCore;

        let cipher = aes_gcm::Aes256Gcm::new(&self.key);
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);

        let mut buf = data.to_vec();
        let tag = cipher
            .encrypt_in_place_detached(&nonce, b"", &mut buf)
            .map_err(|e| BlobError::Encryption(e.to_string()))?;

        // Format: [nonce (12 B)][ciphertext][tag (16 B)]
        let mut out = Vec::with_capacity(12 + buf.len() + 16);
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&buf);
        out.extend_from_slice(&tag);

        self.inner.put(key, Bytes::from(out)).await
    }

    async fn get(&self, key: &str) -> Result<Bytes> {
        use aes_gcm::{AeadInPlace, KeyInit, Nonce, Tag};

        let raw = self.inner.get(key).await?;
        if raw.len() < 28 {
            return Err(BlobError::Encryption("blob too short to decrypt".into()));
        }

        let (nonce_bytes, rest) = raw.split_at(12);
        let (ciphertext, tag_bytes) = rest.split_at(rest.len() - 16);

        let cipher = aes_gcm::Aes256Gcm::new(&self.key);
        let nonce_arr: [u8; 12] = nonce_bytes.try_into().expect("split_at yields 12 bytes");
        let tag_arr: [u8; 16] = tag_bytes.try_into().expect("split_at yields 16 bytes");
        let nonce = Nonce::from(nonce_arr);
        let tag = Tag::from(tag_arr);

        let mut buf = ciphertext.to_vec();
        cipher
            .decrypt_in_place_detached(&nonce, b"", &mut buf, &tag)
            .map_err(|e| BlobError::Encryption(e.to_string()))?;

        Ok(Bytes::from(buf))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.inner.delete(key).await
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        self.inner.exists(key).await
    }
}

// ── Factory ───────────────────────────────────────────────────────────────────

pub fn create_blob_store() -> Arc<dyn BlobStore> {
    let backend = std::env::var("BLOB_BACKEND").unwrap_or_else(|_| "local".to_string());

    let base_store: Arc<dyn BlobStore> = match backend.as_str() {
        "s3" => Arc::new(S3BlobStore::from_env().unwrap_or_else(|e| panic!("{e}"))),
        _ => {
            let path = std::env::var("BLOB_LOCAL_PATH").unwrap_or_else(|_| "./data/blobs".into());
            Arc::new(LocalBlobStore::new(path))
        }
    };

    let encryption_enabled = std::env::var("BLOB_ENCRYPTION")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if encryption_enabled {
        let hex_key = std::env::var("BLOB_ENCRYPTION_KEY").unwrap_or_else(|_| {
            panic!("BLOB_ENCRYPTION=true requires BLOB_ENCRYPTION_KEY (64 hex chars / 32 bytes)")
        });
        let key_bytes = hex::decode(&hex_key).unwrap_or_else(|_| {
            panic!("BLOB_ENCRYPTION_KEY must be 64 valid hex characters (32 bytes)")
        });
        if key_bytes.len() != 32 {
            panic!(
                "BLOB_ENCRYPTION_KEY must be exactly 64 hex chars (32 bytes), got {} bytes",
                key_bytes.len()
            );
        }
        let key_arr: &[u8; 32] = key_bytes.as_slice().try_into().unwrap();
        Arc::new(EncryptingBlobStore::new(base_store, key_arr))
    } else {
        base_store
    }
}
