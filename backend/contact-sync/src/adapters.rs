use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use reqwest::{header, Client, RequestBuilder, StatusCode};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;

use crate::{
    apply_auth, contact_to_vcard, dav, first_href_in_elem, parse_vcard, resolve,
    ContactBookIdentity, ContactChangePage, ContactTombstone, DavAuth, GroupMembership,
    ParsedContact, ProviderError, ProviderErrorCategory, RemoteContact, RemotePhoto,
};

pub(crate) const PROVIDER_API_DISABLED_MESSAGE: &str = "contact provider API is not enabled";

const GOOGLE_PERSON_FIELDS: &str = "names,emailAddresses,phoneNumbers,addresses,organizations,biographies,photos,memberships,metadata";
const GOOGLE_UPDATE_FIELDS: &str =
    "names,emailAddresses,phoneNumbers,addresses,organizations,biographies,memberships";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationResult {
    pub remote_id: String,
    pub remote_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoPayload {
    pub bytes: Vec<u8>,
    pub content_type: String,
    pub version: Option<String>,
}

#[async_trait]
pub trait ContactProviderAdapter: Send + Sync {
    async fn books(&self) -> Result<Vec<ContactBookIdentity>, ProviderError>;

    async fn changes(
        &self,
        book: &ContactBookIdentity,
        cursor: Option<&str>,
        continuation: Option<&str>,
    ) -> Result<ContactChangePage, ProviderError>;

    async fn create(
        &self,
        book: &ContactBookIdentity,
        contact: &ParsedContact,
    ) -> Result<MutationResult, ProviderError>;

    async fn update(
        &self,
        book: &ContactBookIdentity,
        remote_id: &str,
        remote_version: Option<&str>,
        contact: &ParsedContact,
    ) -> Result<MutationResult, ProviderError>;

    async fn delete(
        &self,
        book: &ContactBookIdentity,
        remote_id: &str,
        remote_version: Option<&str>,
    ) -> Result<(), ProviderError>;

    async fn photo(
        &self,
        book: &ContactBookIdentity,
        remote_id: &str,
        photo_reference: Option<&str>,
    ) -> Result<Option<PhotoPayload>, ProviderError>;

    async fn update_photo(
        &self,
        book: &ContactBookIdentity,
        remote_id: &str,
        remote_version: Option<&str>,
        photo: &PhotoPayload,
    ) -> Result<MutationResult, ProviderError>;
}

pub struct CardDavAdapter {
    client: Client,
    base_url: String,
    auth: DavAuth,
}

impl CardDavAdapter {
    pub fn new(client: Client, base_url: impl Into<String>, auth: DavAuth) -> Self {
        Self {
            client,
            base_url: base_url.into(),
            auth,
        }
    }

    pub fn with_tls_options(
        base_url: impl Into<String>,
        auth: DavAuth,
        trusted_cert_der: Option<&[u8]>,
        accept_invalid_tls: bool,
    ) -> Result<Self, ProviderError> {
        let client = crate::client_with_tls_options(trusted_cert_der, accept_invalid_tls)
            .map_err(|_| invalid_response("CardDAV TLS configuration was invalid"))?;
        Ok(Self::new(client, base_url, auth))
    }

    async fn addressbook_home(&self) -> Result<String, ProviderError> {
        let principal_xml = dav(
            &self.client,
            "PROPFIND",
            &self.base_url,
            &self.auth,
            "0",
            "application/xml; charset=utf-8",
            r#"<d:propfind xmlns:d="DAV:"><d:prop><d:current-user-principal/></d:prop></d:propfind>"#,
        )
        .await
        .map_err(carddav_error)?;
        let principal_href = first_href_in_elem(&principal_xml, b"current-user-principal")
            .ok_or_else(|| invalid_response("CardDAV principal was not returned"))?;
        let principal = resolve(&self.base_url, &principal_href)
            .ok_or_else(|| invalid_response("CardDAV principal URL was invalid"))?;
        let home_xml = dav(
            &self.client,
            "PROPFIND",
            &principal,
            &self.auth,
            "0",
            "application/xml; charset=utf-8",
            r#"<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav"><d:prop><c:addressbook-home-set/></d:prop></d:propfind>"#,
        )
        .await
        .map_err(carddav_error)?;
        let home_href = first_href_in_elem(&home_xml, b"addressbook-home-set")
            .ok_or_else(|| invalid_response("CardDAV address-book home was not returned"))?;
        resolve(&self.base_url, &home_href)
            .ok_or_else(|| invalid_response("CardDAV address-book home URL was invalid"))
    }

    async fn conditional_request(
        &self,
        method: reqwest::Method,
        url: &str,
        version: Option<&str>,
        body: Option<String>,
        create: bool,
    ) -> Result<MutationResult, ProviderError> {
        let mut request = apply_auth(self.client.request(method, url), &self.auth);
        if create {
            request = request.header(header::IF_NONE_MATCH, "*");
        } else if let Some(version) = version {
            request = request.header(header::IF_MATCH, version);
        }
        if let Some(body) = body {
            request = request
                .header(header::CONTENT_TYPE, "text/vcard; charset=utf-8")
                .body(body);
        }
        let response = request.send().await.map_err(transport_error)?;
        if response.status() == StatusCode::PRECONDITION_FAILED {
            return Err(provider_error(
                ProviderErrorCategory::Conflict,
                "CardDAV contact changed remotely",
                None,
            ));
        }
        let response = checked_response(response).await?;
        let remote_version = response
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        Ok(MutationResult {
            remote_id: url.to_owned(),
            remote_version,
        })
    }
}

#[async_trait]
impl ContactProviderAdapter for CardDavAdapter {
    async fn books(&self) -> Result<Vec<ContactBookIdentity>, ProviderError> {
        let home = self.addressbook_home().await?;
        let xml = dav(
            &self.client,
            "PROPFIND",
            &home,
            &self.auth,
            "1",
            "application/xml; charset=utf-8",
            r#"<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav"><d:prop><d:displayname/><d:resourcetype/><d:sync-token/><d:supported-report-set/></d:prop></d:propfind>"#,
        )
        .await
        .map_err(carddav_error)?;
        let books = parse_carddav_books(&xml, &self.base_url);
        if books.is_empty() {
            return Err(invalid_response("no CardDAV address books were discovered"));
        }
        Ok(books)
    }

    async fn changes(
        &self,
        book: &ContactBookIdentity,
        cursor: Option<&str>,
        continuation: Option<&str>,
    ) -> Result<ContactChangePage, ProviderError> {
        if continuation.is_some() {
            return Err(invalid_response("CardDAV does not use page continuations"));
        }
        let (body, incremental) = if let Some(cursor) = cursor {
            (
                format!(
                    r#"<d:sync-collection xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav"><d:sync-token>{}</d:sync-token><d:sync-level>1</d:sync-level><d:prop><d:getetag/><c:address-data/></d:prop></d:sync-collection>"#,
                    xml_escape(cursor)
                ),
                true,
            )
        } else {
            (
                r#"<c:addressbook-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav"><d:prop><d:getetag/><c:address-data/></d:prop></c:addressbook-query>"#.to_owned(),
                false,
            )
        };
        let response = dav(
            &self.client,
            "REPORT",
            &book.remote_id,
            &self.auth,
            "1",
            "application/xml; charset=utf-8",
            body,
        )
        .await;
        let xml = match response {
            Ok(xml) => xml,
            Err(_) if incremental => {
                return Err(provider_error(
                    ProviderErrorCategory::CursorExpired,
                    "CardDAV sync token is no longer valid",
                    None,
                ))
            }
            Err(error) => return Err(carddav_error(error)),
        };
        Ok(parse_carddav_changes(&xml, &book.remote_id))
    }

    async fn create(
        &self,
        book: &ContactBookIdentity,
        contact: &ParsedContact,
    ) -> Result<MutationResult, ProviderError> {
        let url = Url::parse(&book.remote_id)
            .and_then(|url| url.join(&format!("{}.vcf", Uuid::new_v4())))
            .map_err(|_| invalid_response("CardDAV book URL was invalid"))?;
        self.conditional_request(
            reqwest::Method::PUT,
            url.as_str(),
            None,
            Some(carddav_vcard(contact)),
            true,
        )
        .await
    }

    async fn update(
        &self,
        _book: &ContactBookIdentity,
        remote_id: &str,
        remote_version: Option<&str>,
        contact: &ParsedContact,
    ) -> Result<MutationResult, ProviderError> {
        self.conditional_request(
            reqwest::Method::PUT,
            remote_id,
            remote_version,
            Some(carddav_vcard(contact)),
            false,
        )
        .await
    }

    async fn delete(
        &self,
        _book: &ContactBookIdentity,
        remote_id: &str,
        remote_version: Option<&str>,
    ) -> Result<(), ProviderError> {
        let mut request = apply_auth(self.client.delete(remote_id), &self.auth);
        if let Some(version) = remote_version {
            request = request.header(header::IF_MATCH, version);
        }
        let response = request.send().await.map_err(transport_error)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        if response.status() == StatusCode::PRECONDITION_FAILED {
            return Err(provider_error(
                ProviderErrorCategory::Conflict,
                "CardDAV contact changed remotely",
                None,
            ));
        }
        checked_response(response).await?;
        Ok(())
    }

    async fn photo(
        &self,
        _book: &ContactBookIdentity,
        _remote_id: &str,
        photo_reference: Option<&str>,
    ) -> Result<Option<PhotoPayload>, ProviderError> {
        let Some(reference) = photo_reference else {
            return Ok(None);
        };
        if reference.starts_with("http://") || reference.starts_with("https://") {
            let response = apply_auth(self.client.get(reference), &self.auth)
                .send()
                .await
                .map_err(transport_error)?;
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(None);
            }
            return response_photo(response).await.map(Some);
        }
        Ok(None)
    }

    async fn update_photo(
        &self,
        _book: &ContactBookIdentity,
        remote_id: &str,
        remote_version: Option<&str>,
        photo: &PhotoPayload,
    ) -> Result<MutationResult, ProviderError> {
        let response = apply_auth(self.client.get(remote_id), &self.auth)
            .send()
            .await
            .map_err(transport_error)?;
        let current = checked_response(response)
            .await?
            .text()
            .await
            .map_err(|_| invalid_response("CardDAV contact vCard was invalid"))?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&photo.bytes);
        let mut lines = current
            .replace("\r\n", "\n")
            .lines()
            .filter(|line| !line.to_ascii_uppercase().starts_with("PHOTO"))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let photo_line = format!("PHOTO;MEDIATYPE={}:{}", photo.content_type, encoded);
        let position = lines
            .iter()
            .position(|line| line.eq_ignore_ascii_case("END:VCARD"))
            .unwrap_or(lines.len());
        lines.insert(position, photo_line);
        self.conditional_request(
            reqwest::Method::PUT,
            remote_id,
            remote_version,
            Some(lines.join("\r\n")),
            false,
        )
        .await
    }
}

pub struct GooglePeopleAdapter {
    client: Client,
    base_url: String,
    access_token: String,
    mutation_lock: Arc<Mutex<()>>,
}

impl GooglePeopleAdapter {
    pub fn new(access_token: impl Into<String>) -> Self {
        Self::with_base_url("https://people.googleapis.com/v1", access_token)
    }

    pub fn with_base_url(base_url: impl Into<String>, access_token: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            access_token: access_token.into(),
            mutation_lock: Arc::new(Mutex::new(())),
        }
    }

    fn authorized(&self, request: RequestBuilder) -> RequestBuilder {
        request.bearer_auth(&self.access_token)
    }

    fn connections_url(
        &self,
        cursor: Option<&str>,
        continuation: Option<&str>,
    ) -> Result<Url, ProviderError> {
        let mut url = Url::parse(&format!("{}/people/me/connections", self.base_url))
            .map_err(|_| invalid_response("Google People endpoint was invalid"))?;
        url.query_pairs_mut()
            .append_pair("personFields", GOOGLE_PERSON_FIELDS)
            .append_pair("pageSize", "1000")
            .append_pair("requestSyncToken", "true")
            .append_pair("sources", "READ_SOURCE_TYPE_CONTACT");
        if let Some(cursor) = cursor {
            url.query_pairs_mut().append_pair("syncToken", cursor);
        }
        if let Some(continuation) = continuation {
            url.query_pairs_mut().append_pair("pageToken", continuation);
        }
        Ok(url)
    }

    async fn mutation_response(
        &self,
        request: RequestBuilder,
        fallback_id: &str,
    ) -> Result<MutationResult, ProviderError> {
        let response = request.send().await.map_err(transport_error)?;
        let response = checked_response(response).await?;
        let value: Value = response
            .json()
            .await
            .map_err(|_| invalid_response("Google People returned invalid JSON"))?;
        Ok(MutationResult {
            remote_id: value["resourceName"]
                .as_str()
                .unwrap_or(fallback_id)
                .to_owned(),
            remote_version: value["etag"].as_str().map(str::to_owned),
        })
    }
}

#[async_trait]
impl ContactProviderAdapter for GooglePeopleAdapter {
    async fn books(&self) -> Result<Vec<ContactBookIdentity>, ProviderError> {
        let mut next = Some(format!("{}/contactGroups?pageSize=1000", self.base_url));
        let mut books = Vec::new();
        while let Some(url) = next.take() {
            let response = self
                .authorized(self.client.get(url))
                .send()
                .await
                .map_err(transport_error)?;
            let value: Value = checked_json(response).await?;
            books.extend(
                value["contactGroups"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(google_group_book),
            );
            next = value["nextPageToken"].as_str().map(|token| {
                format!(
                    "{}/contactGroups?pageSize=1000&pageToken={token}",
                    self.base_url
                )
            });
        }
        if books.is_empty() {
            books.push(google_default_book());
        }
        Ok(books)
    }

    async fn changes(
        &self,
        book: &ContactBookIdentity,
        cursor: Option<&str>,
        continuation: Option<&str>,
    ) -> Result<ContactChangePage, ProviderError> {
        let response = self
            .authorized(self.client.get(self.connections_url(cursor, continuation)?))
            .send()
            .await
            .map_err(transport_error)?;
        if response.status() == StatusCode::GONE {
            return Err(provider_error(
                ProviderErrorCategory::CursorExpired,
                "Google People sync token expired",
                None,
            ));
        }
        let value: Value = checked_json(response).await?;
        Ok(parse_google_page(&value, &book.remote_id))
    }

    async fn create(
        &self,
        _book: &ContactBookIdentity,
        contact: &ParsedContact,
    ) -> Result<MutationResult, ProviderError> {
        let _guard = self.mutation_lock.lock().await;
        self.mutation_response(
            self.authorized(
                self.client
                    .post(format!("{}/people:createContact", self.base_url))
                    .json(&google_contact_body(contact, None)),
            ),
            &contact.uid,
        )
        .await
    }

    async fn update(
        &self,
        _book: &ContactBookIdentity,
        remote_id: &str,
        remote_version: Option<&str>,
        contact: &ParsedContact,
    ) -> Result<MutationResult, ProviderError> {
        let _guard = self.mutation_lock.lock().await;
        self.mutation_response(
            self.authorized(
                self.client
                    .patch(format!("{}/{remote_id}:updateContact", self.base_url))
                    .query(&[("updatePersonFields", GOOGLE_UPDATE_FIELDS)])
                    .json(&google_contact_body(contact, remote_version)),
            ),
            remote_id,
        )
        .await
    }

    async fn delete(
        &self,
        _book: &ContactBookIdentity,
        remote_id: &str,
        _remote_version: Option<&str>,
    ) -> Result<(), ProviderError> {
        let _guard = self.mutation_lock.lock().await;
        let response = self
            .authorized(
                self.client
                    .delete(format!("{}/{remote_id}:deleteContact", self.base_url)),
            )
            .send()
            .await
            .map_err(transport_error)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        checked_response(response).await?;
        Ok(())
    }

    async fn photo(
        &self,
        _book: &ContactBookIdentity,
        _remote_id: &str,
        photo_reference: Option<&str>,
    ) -> Result<Option<PhotoPayload>, ProviderError> {
        let Some(reference) = photo_reference else {
            return Ok(None);
        };
        let response = self
            .authorized(self.client.get(reference))
            .send()
            .await
            .map_err(transport_error)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        response_photo(response).await.map(Some)
    }

    async fn update_photo(
        &self,
        _book: &ContactBookIdentity,
        remote_id: &str,
        remote_version: Option<&str>,
        photo: &PhotoPayload,
    ) -> Result<MutationResult, ProviderError> {
        let _guard = self.mutation_lock.lock().await;
        self.mutation_response(
            self.authorized(
                self.client
                    .patch(format!("{}/{remote_id}:updateContactPhoto", self.base_url))
                    .json(&json!({
                        "photoBytes": base64::engine::general_purpose::STANDARD.encode(&photo.bytes),
                        "personFields": "photos",
                        "etag": remote_version
                    })),
            ),
            remote_id,
        )
        .await
    }
}

pub struct MicrosoftGraphAdapter {
    client: Client,
    base_url: String,
    access_token: String,
}

impl MicrosoftGraphAdapter {
    pub fn new(access_token: impl Into<String>) -> Self {
        Self::with_base_url("https://graph.microsoft.com/v1.0", access_token)
    }

    pub fn with_base_url(base_url: impl Into<String>, access_token: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            access_token: access_token.into(),
        }
    }

    fn authorized(&self, request: RequestBuilder) -> RequestBuilder {
        request.bearer_auth(&self.access_token)
    }

    fn contacts_collection(&self, book: &ContactBookIdentity) -> String {
        if book.is_default {
            format!("{}/me/contacts", self.base_url)
        } else {
            format!(
                "{}/me/contactFolders/{}/contacts",
                self.base_url, book.remote_id
            )
        }
    }

    async fn mutation_response(
        &self,
        response: reqwest::Response,
        fallback_id: &str,
    ) -> Result<MutationResult, ProviderError> {
        let value: Value = checked_json(response).await?;
        Ok(MutationResult {
            remote_id: value["id"].as_str().unwrap_or(fallback_id).to_owned(),
            remote_version: graph_version(&value),
        })
    }
}

#[async_trait]
impl ContactProviderAdapter for MicrosoftGraphAdapter {
    async fn books(&self) -> Result<Vec<ContactBookIdentity>, ProviderError> {
        let mut books = vec![graph_default_book()];
        let mut next = Some(format!(
            "{}/me/contactFolders?$top=100&$expand=childFolders",
            self.base_url
        ));
        while let Some(url) = next.take() {
            let value: Value = checked_json(
                self.authorized(self.client.get(url))
                    .send()
                    .await
                    .map_err(transport_error)?,
            )
            .await?;
            for folder in value["value"].as_array().into_iter().flatten() {
                append_graph_folders(folder, None, &mut books);
            }
            next = value["@odata.nextLink"].as_str().map(str::to_owned);
        }
        Ok(books)
    }

    async fn changes(
        &self,
        book: &ContactBookIdentity,
        cursor: Option<&str>,
        continuation: Option<&str>,
    ) -> Result<ContactChangePage, ProviderError> {
        let url = continuation
            .or(cursor)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{}/delta", self.contacts_collection(book)));
        let value: Value = checked_json(
            self.authorized(self.client.get(url))
                .send()
                .await
                .map_err(transport_error)?,
        )
        .await?;
        Ok(parse_graph_page(&value, &book.remote_id))
    }

    async fn create(
        &self,
        book: &ContactBookIdentity,
        contact: &ParsedContact,
    ) -> Result<MutationResult, ProviderError> {
        let response = self
            .authorized(
                self.client
                    .post(self.contacts_collection(book))
                    .json(&graph_contact_body(contact)),
            )
            .send()
            .await
            .map_err(transport_error)?;
        self.mutation_response(response, &contact.uid).await
    }

    async fn update(
        &self,
        book: &ContactBookIdentity,
        remote_id: &str,
        remote_version: Option<&str>,
        contact: &ParsedContact,
    ) -> Result<MutationResult, ProviderError> {
        let mut request = self.authorized(
            self.client
                .patch(format!("{}/{remote_id}", self.contacts_collection(book)))
                .json(&graph_contact_body(contact)),
        );
        if let Some(version) = remote_version {
            request = request.header(header::IF_MATCH, version);
        }
        let response = request.send().await.map_err(transport_error)?;
        self.mutation_response(response, remote_id).await
    }

    async fn delete(
        &self,
        book: &ContactBookIdentity,
        remote_id: &str,
        remote_version: Option<&str>,
    ) -> Result<(), ProviderError> {
        let mut request = self.authorized(
            self.client
                .delete(format!("{}/{remote_id}", self.contacts_collection(book))),
        );
        if let Some(version) = remote_version {
            request = request.header(header::IF_MATCH, version);
        }
        let response = request.send().await.map_err(transport_error)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        checked_response(response).await?;
        Ok(())
    }

    async fn photo(
        &self,
        book: &ContactBookIdentity,
        remote_id: &str,
        _photo_reference: Option<&str>,
    ) -> Result<Option<PhotoPayload>, ProviderError> {
        let response = self
            .authorized(self.client.get(format!(
                "{}/{remote_id}/photo/$value",
                self.contacts_collection(book)
            )))
            .send()
            .await
            .map_err(transport_error)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        response_photo(response).await.map(Some)
    }

    async fn update_photo(
        &self,
        book: &ContactBookIdentity,
        remote_id: &str,
        remote_version: Option<&str>,
        photo: &PhotoPayload,
    ) -> Result<MutationResult, ProviderError> {
        let mut request = self.authorized(
            self.client
                .put(format!(
                    "{}/{remote_id}/photo/$value",
                    self.contacts_collection(book)
                ))
                .header(header::CONTENT_TYPE, &photo.content_type)
                .body(photo.bytes.clone()),
        );
        if let Some(version) = remote_version {
            request = request.header(header::IF_MATCH, version);
        }
        let response = checked_response(request.send().await.map_err(transport_error)?).await?;
        Ok(MutationResult {
            remote_id: remote_id.to_owned(),
            remote_version: response
                .headers()
                .get(header::ETAG)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned),
        })
    }
}

async fn checked_json(response: reqwest::Response) -> Result<Value, ProviderError> {
    checked_response(response)
        .await?
        .json()
        .await
        .map_err(|_| invalid_response("contact provider returned invalid JSON"))
}

async fn checked_response(response: reqwest::Response) -> Result<reqwest::Response, ProviderError> {
    let status = response.status();
    if status.is_success() || status == StatusCode::MULTI_STATUS {
        return Ok(response);
    }
    let retry_after_seconds = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    let forbidden_body = if status == StatusCode::FORBIDDEN {
        response.json::<Value>().await.ok()
    } else {
        None
    };
    if let Some(body) = forbidden_body.as_ref() {
        if provider_api_is_disabled(body) {
            return Err(provider_error(
                ProviderErrorCategory::Unavailable,
                PROVIDER_API_DISABLED_MESSAGE,
                retry_after_seconds,
            ));
        }
    }
    let category = match status {
        StatusCode::UNAUTHORIZED => ProviderErrorCategory::ReauthenticationRequired,
        StatusCode::FORBIDDEN => ProviderErrorCategory::ConsentRequired,
        StatusCode::CONFLICT | StatusCode::PRECONDITION_FAILED => ProviderErrorCategory::Conflict,
        StatusCode::TOO_MANY_REQUESTS => ProviderErrorCategory::RateLimited,
        StatusCode::GONE => ProviderErrorCategory::CursorExpired,
        status if status.is_server_error() => ProviderErrorCategory::Transport,
        _ => ProviderErrorCategory::InvalidResponse,
    };
    Err(provider_error(
        category,
        format!("contact provider request failed with HTTP {status}"),
        retry_after_seconds,
    ))
}

fn provider_api_is_disabled(value: &Value) -> bool {
    json_contains(value, "SERVICE_DISABLED")
        || json_contains(value, "has not been used in project")
        || json_contains(value, "api is disabled")
}

fn json_contains(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(value) => value
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase()),
        Value::Array(values) => values.iter().any(|value| json_contains(value, needle)),
        Value::Object(values) => values.values().any(|value| json_contains(value, needle)),
        _ => false,
    }
}

async fn response_photo(response: reqwest::Response) -> Result<PhotoPayload, ProviderError> {
    let response = checked_response(response).await?;
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_owned();
    let version = response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = response
        .bytes()
        .await
        .map_err(|_| invalid_response("contact provider returned an invalid photo"))?
        .to_vec();
    Ok(PhotoPayload {
        bytes,
        content_type,
        version,
    })
}

fn transport_error(_error: reqwest::Error) -> ProviderError {
    provider_error(
        ProviderErrorCategory::Transport,
        "contact provider request failed",
        None,
    )
}

fn carddav_error(error: String) -> ProviderError {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("certificate")
        || normalized.contains("invalid peer")
        || normalized.contains("notvalidforname")
        || normalized.contains("tls")
    {
        return provider_error(
            ProviderErrorCategory::Unavailable,
            "CardDAV TLS verification failed",
            None,
        );
    }
    if normalized.contains("-> 401") || normalized.contains("-> 403") {
        return provider_error(
            ProviderErrorCategory::Authentication,
            "CardDAV credentials were rejected",
            None,
        );
    }
    if normalized.contains("-> 404") {
        return provider_error(
            ProviderErrorCategory::Unavailable,
            "CardDAV endpoint was not found",
            None,
        );
    }
    if normalized.contains("connection failed") || normalized.contains("timed out") {
        return provider_error(
            ProviderErrorCategory::Transport,
            "CardDAV server could not be reached",
            None,
        );
    }
    provider_error(
        ProviderErrorCategory::InvalidResponse,
        "CardDAV server returned an unsupported response",
        None,
    )
}

fn invalid_response(message: impl Into<String>) -> ProviderError {
    provider_error(ProviderErrorCategory::InvalidResponse, message, None)
}

fn provider_error(
    category: ProviderErrorCategory,
    message: impl Into<String>,
    retry_after_seconds: Option<u64>,
) -> ProviderError {
    ProviderError {
        category,
        message: message.into(),
        retry_after_seconds,
    }
}

fn google_default_book() -> ContactBookIdentity {
    ContactBookIdentity {
        remote_id: "contactGroups/myContacts".to_owned(),
        display_name: "Contacts".to_owned(),
        parent_remote_id: None,
        is_default: true,
        is_writable: true,
        provider_metadata: Value::Null,
    }
}

fn google_group_book(value: &Value) -> Option<ContactBookIdentity> {
    let remote_id = value["resourceName"].as_str()?.to_owned();
    Some(ContactBookIdentity {
        display_name: value["name"].as_str().unwrap_or(&remote_id).to_owned(),
        is_default: remote_id == "contactGroups/myContacts",
        is_writable: value["groupType"].as_str() != Some("SYSTEM_CONTACT_GROUP")
            || remote_id == "contactGroups/myContacts",
        remote_id,
        parent_remote_id: None,
        provider_metadata: value.clone(),
    })
}

fn parse_google_page(value: &Value, book_remote_id: &str) -> ContactChangePage {
    let mut page = ContactChangePage {
        continuation: value["nextPageToken"].as_str().map(str::to_owned),
        final_cursor: value["nextSyncToken"].as_str().map(str::to_owned),
        ..Default::default()
    };
    for person in value["connections"].as_array().into_iter().flatten() {
        let remote_id = person["resourceName"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if remote_id.is_empty() {
            continue;
        }
        if person["metadata"]["deleted"].as_bool().unwrap_or(false) {
            page.tombstones.push(ContactTombstone {
                book_remote_id: book_remote_id.to_owned(),
                remote_id,
            });
            continue;
        }
        page.upserts.push(RemoteContact {
            book_remote_id: book_remote_id.to_owned(),
            remote_id,
            remote_version: person["etag"].as_str().map(str::to_owned),
            contact: crate::google_contact(person),
            photo: person["photos"]
                .as_array()
                .and_then(|photos| photos.first())
                .and_then(|photo| photo["url"].as_str())
                .map(|reference| RemotePhoto {
                    reference: reference.to_owned(),
                    version: person["etag"].as_str().map(str::to_owned),
                    content_type: None,
                }),
            groups: person["memberships"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|membership| {
                    membership["contactGroupMembership"]["contactGroupResourceName"]
                        .as_str()
                        .map(|remote_group_id| GroupMembership {
                            remote_group_id: remote_group_id.to_owned(),
                            display_name: None,
                        })
                })
                .collect(),
            provider_metadata: person["metadata"].clone(),
        });
    }
    page
}

fn google_contact_body(contact: &ParsedContact, etag: Option<&str>) -> Value {
    let mut body = crate::to_google_contact(contact);
    if let Some(etag) = etag {
        body["etag"] = json!(etag);
    }
    body
}

fn graph_default_book() -> ContactBookIdentity {
    ContactBookIdentity {
        remote_id: "default".to_owned(),
        display_name: "Contacts".to_owned(),
        parent_remote_id: None,
        is_default: true,
        is_writable: true,
        provider_metadata: Value::Null,
    }
}

fn append_graph_folders(
    value: &Value,
    parent_remote_id: Option<&str>,
    books: &mut Vec<ContactBookIdentity>,
) {
    let Some(remote_id) = value["id"].as_str() else {
        return;
    };
    books.push(ContactBookIdentity {
        remote_id: remote_id.to_owned(),
        display_name: value["displayName"]
            .as_str()
            .unwrap_or(remote_id)
            .to_owned(),
        parent_remote_id: parent_remote_id
            .map(str::to_owned)
            .or_else(|| value["parentFolderId"].as_str().map(str::to_owned)),
        is_default: false,
        is_writable: true,
        provider_metadata: value.clone(),
    });
    for child in value["childFolders"].as_array().into_iter().flatten() {
        append_graph_folders(child, Some(remote_id), books);
    }
}

fn parse_graph_page(value: &Value, book_remote_id: &str) -> ContactChangePage {
    let mut page = ContactChangePage {
        continuation: value["@odata.nextLink"].as_str().map(str::to_owned),
        final_cursor: value["@odata.deltaLink"].as_str().map(str::to_owned),
        ..Default::default()
    };
    for contact in value["value"].as_array().into_iter().flatten() {
        let remote_id = contact["id"].as_str().unwrap_or_default().to_owned();
        if remote_id.is_empty() {
            continue;
        }
        if !contact["@removed"].is_null() {
            page.tombstones.push(ContactTombstone {
                book_remote_id: book_remote_id.to_owned(),
                remote_id,
            });
        } else {
            page.upserts.push(RemoteContact {
                book_remote_id: book_remote_id.to_owned(),
                remote_id: remote_id.clone(),
                remote_version: graph_version(contact),
                contact: crate::graph_contact(contact),
                photo: Some(RemotePhoto {
                    reference: format!("{remote_id}/photo/$value"),
                    version: graph_version(contact),
                    content_type: None,
                }),
                groups: Vec::new(),
                provider_metadata: contact.clone(),
            });
        }
    }
    page
}

fn graph_version(value: &Value) -> Option<String> {
    value["@odata.etag"]
        .as_str()
        .or_else(|| value["changeKey"].as_str())
        .map(str::to_owned)
}

fn graph_contact_body(contact: &ParsedContact) -> Value {
    crate::to_graph_contact(contact)
}

fn carddav_vcard(contact: &ParsedContact) -> String {
    let generated = contact_to_vcard(contact);
    let Some(raw) = contact.raw_vcard.as_deref() else {
        return generated;
    };
    let managed = [
        "BEGIN", "VERSION", "UID", "FN", "N", "ORG", "TITLE", "EMAIL", "TEL", "ADR", "NOTE", "END",
    ];
    let preserved = raw
        .replace("\r\n", "\n")
        .lines()
        .filter(|line| {
            let property = line
                .split_once(':')
                .map(|(key, _)| key)
                .unwrap_or(line)
                .split(';')
                .next()
                .unwrap_or_default()
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            !managed.contains(&property.as_str())
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if preserved.is_empty() {
        return generated;
    }
    let mut lines = generated.lines().map(str::to_owned).collect::<Vec<_>>();
    let position = lines
        .iter()
        .position(|line| line.eq_ignore_ascii_case("END:VCARD"))
        .unwrap_or(lines.len());
    lines.splice(position..position, preserved);
    lines.join("\r\n")
}

fn parse_carddav_books(xml: &str, base_url: &str) -> Vec<ContactBookIdentity> {
    xml_responses(xml)
        .into_iter()
        .filter(|response| response.contains("addressbook"))
        .filter_map(|response| {
            let href = xml_text(&response, "href")?;
            let remote_id = resolve(base_url, &href)?;
            Some(ContactBookIdentity {
                display_name: xml_text(&response, "displayname").unwrap_or_else(|| href.clone()),
                remote_id,
                parent_remote_id: None,
                is_default: false,
                is_writable: !response.contains("read-only"),
                provider_metadata: json!({
                    "supports_sync_collection": response.contains("sync-collection")
                }),
            })
        })
        .collect()
}

fn parse_carddav_changes(xml: &str, book_url: &str) -> ContactChangePage {
    let mut page = ContactChangePage {
        final_cursor: xml_text(xml, "sync-token").or_else(|| Some("full-query".to_owned())),
        ..Default::default()
    };
    for response in xml_responses(xml) {
        let Some(href) = xml_text(&response, "href") else {
            continue;
        };
        let remote_id = resolve(book_url, &href).unwrap_or(href);
        if response.contains(" 404 ") || response.contains(">404<") {
            page.tombstones.push(ContactTombstone {
                book_remote_id: book_url.to_owned(),
                remote_id,
            });
            continue;
        }
        let Some(raw_vcard) = xml_text(&response, "address-data") else {
            continue;
        };
        let Some(contact) = parse_vcard(&raw_vcard) else {
            continue;
        };
        let remote_version = xml_text(&response, "getetag");
        let photo = contact
            .photo_reference
            .as_ref()
            .map(|reference| RemotePhoto {
                reference: reference.clone(),
                version: remote_version.clone(),
                content_type: None,
            });
        page.upserts.push(RemoteContact {
            book_remote_id: book_url.to_owned(),
            remote_id,
            remote_version,
            contact,
            photo,
            groups: Vec::new(),
            provider_metadata: Value::Null,
        });
    }
    page
}

fn xml_responses(xml: &str) -> Vec<String> {
    let mut responses = Vec::new();
    let mut remaining = xml;
    while let Some(start) = find_local_tag(remaining, "response", false) {
        let after_start = &remaining[start..];
        let Some(open_end) = after_start.find('>') else {
            break;
        };
        let Some(close_start) = find_local_tag(&after_start[open_end + 1..], "response", true)
        else {
            break;
        };
        let close_start = open_end + 1 + close_start;
        let Some(close_end) = after_start[close_start..].find('>') else {
            break;
        };
        let end = close_start + close_end + 1;
        responses.push(after_start[..end].to_owned());
        remaining = &after_start[end..];
    }
    responses
}

fn xml_text(xml: &str, local_name: &str) -> Option<String> {
    let start = find_local_tag(xml, local_name, false)?;
    let content_start = start + xml[start..].find('>')? + 1;
    let close = find_local_tag(&xml[content_start..], local_name, true)? + content_start;
    let value = &xml[content_start..close];
    let value = value
        .strip_prefix("<![CDATA[")
        .and_then(|value| value.strip_suffix("]]>"))
        .unwrap_or(value);
    Some(
        value
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
            .trim()
            .to_owned(),
    )
}

fn find_local_tag(xml: &str, local_name: &str, closing: bool) -> Option<usize> {
    let prefix = if closing { "</" } else { "<" };
    let mut offset = 0;
    while let Some(index) = xml[offset..].find(prefix) {
        let absolute = offset + index;
        let tag = &xml[absolute + prefix.len()..];
        let name_end = tag
            .find(|character: char| character == '>' || character.is_whitespace())
            .unwrap_or(tag.len());
        let qualified = &tag[..name_end];
        if qualified.rsplit(':').next() == Some(local_name) {
            return Some(absolute);
        }
        offset = absolute + prefix.len();
    }
    None
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn book(remote_id: &str) -> ContactBookIdentity {
        ContactBookIdentity {
            remote_id: remote_id.to_owned(),
            display_name: "Contacts".to_owned(),
            parent_remote_id: None,
            is_default: true,
            is_writable: true,
            provider_metadata: Value::Null,
        }
    }

    async fn one_shot_server(
        response: impl Into<String>,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let response = response.into();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let size = socket.read(&mut request).await.unwrap();
            socket.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&request[..size]).into_owned()
        });
        (format!("http://{address}"), task)
    }

    #[test]
    fn carddav_parses_multiple_books_and_tombstones() {
        let discovery = r#"<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav"><d:response><d:href>/books/personal/</d:href><d:propstat><d:prop><d:displayname>Personal</d:displayname><d:resourcetype><c:addressbook/></d:resourcetype><d:supported-report-set><d:sync-collection/></d:supported-report-set></d:prop></d:propstat></d:response><d:response><d:href>/books/team/</d:href><d:propstat><d:prop><d:displayname>Team</d:displayname><d:resourcetype><c:addressbook/></d:resourcetype></d:prop></d:propstat></d:response></d:multistatus>"#;
        let books = parse_carddav_books(discovery, "https://dav.example.com/");
        assert_eq!(books.len(), 2);
        assert_eq!(books[0].display_name, "Personal");
        assert_eq!(books[0].provider_metadata["supports_sync_collection"], true);

        let delta = r#"<d:multistatus xmlns:d="DAV:"><d:response><d:href>gone.vcf</d:href><d:status>HTTP/1.1 404 Not Found</d:status></d:response><d:sync-token>next</d:sync-token></d:multistatus>"#;
        let page = parse_carddav_changes(delta, "https://dav.example.com/books/personal/");
        assert_eq!(page.tombstones.len(), 1);
        assert_eq!(page.final_cursor.as_deref(), Some("next"));
    }

    #[test]
    fn carddav_updates_normalized_fields_without_dropping_extensions_or_photos() {
        let vcard = carddav_vcard(&ParsedContact {
            uid: "one".into(),
            display_name: Some("Updated name".into()),
            raw_vcard: Some(
                "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:one\r\nFN:Old name\r\nPHOTO;MEDIATYPE=image/png:abc\r\nX-CUSTOM:keep\r\nEND:VCARD".into(),
            ),
            ..Default::default()
        });
        assert!(vcard.contains("FN:Updated name"));
        assert!(vcard.contains("PHOTO;MEDIATYPE=image/png:abc"));
        assert!(vcard.contains("X-CUSTOM:keep"));
        assert!(!vcard.contains("FN:Old name"));
    }

    #[test]
    fn carddav_errors_are_actionable_and_retry_safe() {
        let cases = [
            (
                "certificate subjectAltName does not match hostname",
                ProviderErrorCategory::Unavailable,
                "CardDAV TLS verification failed",
            ),
            (
                "https://dav.example.test/carddav/ -> 401 Unauthorized",
                ProviderErrorCategory::Authentication,
                "CardDAV credentials were rejected",
            ),
            (
                "https://dav.example.test/carddav/ -> 404 Not Found",
                ProviderErrorCategory::Unavailable,
                "CardDAV endpoint was not found",
            ),
            (
                "connection failed while contacting DAV server",
                ProviderErrorCategory::Transport,
                "CardDAV server could not be reached",
            ),
            (
                "unexpected multistatus document",
                ProviderErrorCategory::InvalidResponse,
                "CardDAV server returned an unsupported response",
            ),
        ];

        for (raw, expected_category, expected_message) in cases {
            let error = carddav_error(raw.to_owned());
            assert_eq!(error.category, expected_category);
            assert_eq!(error.message, expected_message);
            assert_eq!(error.retry_after_seconds, None);
        }
    }

    #[tokio::test]
    async fn carddav_update_sends_if_match() {
        let (base, server) =
            one_shot_server("HTTP/1.1 204 No Content\r\nETag: \"v2\"\r\nContent-Length: 0\r\n\r\n")
                .await;
        let adapter = CardDavAdapter::new(
            Client::new(),
            &base,
            DavAuth::Basic {
                username: "user".into(),
                password: "password".into(),
            },
        );
        let result = adapter
            .update(
                &book(&base),
                &format!("{base}/contact.vcf"),
                Some("\"v1\""),
                &ParsedContact {
                    uid: "contact".into(),
                    display_name: Some("Contact".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(result.remote_version.as_deref(), Some("\"v2\""));
        let request = server.await.unwrap();
        assert!(request.contains("if-match: \"v1\""));
    }

    #[test]
    fn google_page_preserves_versions_groups_and_deletions() {
        let page = parse_google_page(
            &json!({
                "connections": [
                    {"resourceName": "people/1", "etag": "v1", "names": [{"displayName": "One"}], "memberships": [{"contactGroupMembership": {"contactGroupResourceName": "contactGroups/friends"}}]},
                    {"resourceName": "people/2", "metadata": {"deleted": true}}
                ],
                "nextPageToken": "page-2"
            }),
            "contactGroups/myContacts",
        );
        assert_eq!(page.upserts[0].remote_version.as_deref(), Some("v1"));
        assert_eq!(
            page.upserts[0].groups[0].remote_group_id,
            "contactGroups/friends"
        );
        assert_eq!(page.tombstones[0].remote_id, "people/2");
        assert_eq!(page.continuation.as_deref(), Some("page-2"));
        assert!(page.final_cursor.is_none());
    }

    #[tokio::test]
    async fn google_uses_fixed_parameters_and_classifies_expiry() {
        let (base, server) =
            one_shot_server("HTTP/1.1 410 Gone\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await;
        let adapter = GooglePeopleAdapter::with_base_url(&base, "token");
        let error = adapter
            .changes(&book("contactGroups/myContacts"), Some("cursor"), None)
            .await
            .unwrap_err();
        assert_eq!(error.category, ProviderErrorCategory::CursorExpired);
        let request = server.await.unwrap();
        assert!(request.contains("pageSize=1000"));
        assert!(request.contains("requestSyncToken=true"));
        assert!(request.contains("syncToken=cursor"));
    }

    #[tokio::test]
    async fn google_distinguishes_disabled_api_from_missing_consent() {
        let disabled_body = json!({
            "error": {
                "code": 403,
                "status": "PERMISSION_DENIED",
                "details": [{
                    "@type": "type.googleapis.com/google.rpc.ErrorInfo",
                    "reason": "SERVICE_DISABLED",
                    "domain": "googleapis.com",
                    "metadata": { "service": "people.googleapis.com" }
                }]
            }
        })
        .to_string();
        let disabled_response = format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            disabled_body.len(),
            disabled_body
        );
        let (base, server) = one_shot_server(disabled_response).await;
        let error = GooglePeopleAdapter::with_base_url(&base, "token")
            .books()
            .await
            .unwrap_err();
        assert_eq!(error.category, ProviderErrorCategory::Unavailable);
        assert_eq!(error.message, PROVIDER_API_DISABLED_MESSAGE);
        server.await.unwrap();

        let scope_body = json!({
            "error": {
                "code": 403,
                "message": "Request had insufficient authentication scopes.",
                "status": "PERMISSION_DENIED",
                "details": [{ "reason": "ACCESS_TOKEN_SCOPE_INSUFFICIENT" }]
            }
        })
        .to_string();
        let scope_response = format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            scope_body.len(),
            scope_body
        );
        let (base, server) = one_shot_server(scope_response).await;
        let error = GooglePeopleAdapter::with_base_url(&base, "token")
            .books()
            .await
            .unwrap_err();
        assert_eq!(error.category, ProviderErrorCategory::ConsentRequired);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn google_version_conflict_is_categorized_and_sends_field_mask() {
        let (base, server) = one_shot_server(
            "HTTP/1.1 412 Precondition Failed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .await;
        let adapter = GooglePeopleAdapter::with_base_url(&base, "token");
        let error = adapter
            .update(
                &book("contactGroups/myContacts"),
                "people/one",
                Some("etag-1"),
                &ParsedContact {
                    uid: "people/one".into(),
                    display_name: Some("One".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.category, ProviderErrorCategory::Conflict);
        let request = server.await.unwrap();
        assert!(request.contains("updatePersonFields="));
        assert!(request.contains("etag-1"));
    }

    #[test]
    fn graph_page_keeps_opaque_links_and_removed_contacts() {
        let page = parse_graph_page(
            &json!({
                "value": [
                    {"id": "one", "displayName": "One", "@odata.etag": "v1"},
                    {"id": "gone", "@removed": {"reason": "deleted"}}
                ],
                "@odata.nextLink": "https://opaque.invalid/next?$skiptoken=a%2Bb"
            }),
            "folder",
        );
        assert_eq!(page.upserts[0].remote_version.as_deref(), Some("v1"));
        assert_eq!(page.tombstones[0].remote_id, "gone");
        assert_eq!(
            page.continuation.as_deref(),
            Some("https://opaque.invalid/next?$skiptoken=a%2Bb")
        );
    }

    #[test]
    fn graph_nested_folders_keep_parent_identity() {
        let mut books = Vec::new();
        append_graph_folders(
            &json!({
                "id": "parent",
                "displayName": "Parent",
                "childFolders": [{"id": "child", "displayName": "Child"}]
            }),
            None,
            &mut books,
        );
        assert_eq!(books.len(), 2);
        assert_eq!(books[1].parent_remote_id.as_deref(), Some("parent"));
    }

    #[tokio::test]
    async fn throttling_keeps_retry_after_without_response_body() {
        let (base, server) = one_shot_server(
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 17\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .await;
        let adapter = MicrosoftGraphAdapter::with_base_url(&base, "token");
        let error = adapter
            .changes(&book("default"), None, None)
            .await
            .unwrap_err();
        assert_eq!(error.category, ProviderErrorCategory::RateLimited);
        assert_eq!(error.retry_after_seconds, Some(17));
        let _ = server.await.unwrap();
    }
}
