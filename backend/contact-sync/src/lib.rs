//! Contact sync primitives for CardDAV, Microsoft Graph, and Google People.
//!
//! The API crate owns persistence and credentials. This crate owns provider
//! HTTP calls plus vCard parsing/building into a provider-neutral contact model.

use std::{collections::HashMap, future::Future};

use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{sync::Mutex, task::JoinHandle};
use url::Url;
use vcard4::property::Property;

#[derive(Clone)]
pub enum DavAuth {
    Basic { username: String, password: String },
    Bearer(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LabeledValue {
    pub label: Option<String>,
    pub value: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PostalAddress {
    pub label: Option<String>,
    pub street: Option<String>,
    pub locality: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParsedContact {
    pub uid: String,
    pub display_name: Option<String>,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub org: Option<String>,
    pub title: Option<String>,
    pub emails: Vec<LabeledValue>,
    pub phones: Vec<LabeledValue>,
    pub addresses: Vec<PostalAddress>,
    pub notes: Option<String>,
    pub photo_reference: Option<String>,
    pub raw_vcard: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CardDavContact {
    pub href: String,
    pub etag: Option<String>,
    pub contact: ParsedContact,
}

#[derive(Debug, Clone)]
pub struct GraphSyncPage {
    pub contacts: Vec<ParsedContact>,
    pub delta_link: Option<String>,
    pub next_link: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GoogleSyncPage {
    pub contacts: Vec<ParsedContact>,
    pub sync_token: Option<String>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContactSyncStatus {
    pub state: String,
    pub last_synced_at: Option<String>,
    pub error: Option<String>,
}

impl Default for ContactSyncStatus {
    fn default() -> Self {
        Self {
            state: "idle".to_owned(),
            last_synced_at: None,
            error: None,
        }
    }
}

pub struct ContactSyncManager {
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
    statuses: Mutex<HashMap<String, ContactSyncStatus>>,
}

impl ContactSyncManager {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            statuses: Mutex::new(HashMap::new()),
        }
    }

    pub async fn start_account<F>(&self, account_id: String, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.stop_account(&account_id).await;
        self.statuses.lock().await.insert(
            account_id.clone(),
            ContactSyncStatus {
                state: "syncing".to_owned(),
                ..Default::default()
            },
        );
        let handle_id = account_id.clone();
        let handle = tokio::spawn(async move {
            task.await;
        });
        self.tasks.lock().await.insert(handle_id, handle);
    }

    pub async fn stop_account(&self, account_id: &str) {
        let mut tasks = self.tasks.lock().await;
        if let Some(handle) = tasks.remove(account_id) {
            handle.abort();
        }
        self.statuses.lock().await.remove(account_id);
    }

    pub async fn account_status(&self, account_id: &str) -> ContactSyncStatus {
        self.statuses
            .lock()
            .await
            .get(account_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn update_status(&self, account_id: &str, status: ContactSyncStatus) {
        self.statuses
            .lock()
            .await
            .insert(account_id.to_owned(), status);
    }
}

impl Default for ContactSyncManager {
    fn default() -> Self {
        Self::new()
    }
}

fn client() -> Result<reqwest::Client, String> {
    client_with_tls_options(None, false)
}

fn client_with_tls_options(
    trusted_cert_der: Option<&[u8]>,
    accept_invalid_tls: bool,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30));
    if accept_invalid_tls {
        builder = builder
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true);
    }
    if let Some(der) = trusted_cert_der {
        let cert = reqwest::Certificate::from_der(der)
            .map_err(|e| format!("trusted certificate invalid: {e}"))?;
        builder = builder
            .add_root_certificate(cert)
            .danger_accept_invalid_hostnames(true);
    }
    builder.build().map_err(|e| e.to_string())
}

fn explain_transport_error(err: reqwest::Error) -> String {
    if err.is_timeout() {
        return format!("request timed out: {err}");
    }
    if err.is_connect() {
        return format!("connection failed: {err}");
    }
    format!("request failed: {err}")
}

fn apply_auth(rb: reqwest::RequestBuilder, auth: &DavAuth) -> reqwest::RequestBuilder {
    match auth {
        DavAuth::Basic { username, password } => rb.basic_auth(username, Some(password)),
        DavAuth::Bearer(token) => rb.bearer_auth(token),
    }
}

async fn dav(
    c: &reqwest::Client,
    method: &str,
    url: &str,
    auth: &DavAuth,
    depth: &str,
    content_type: &str,
    body: impl Into<String>,
) -> Result<String, String> {
    let m = Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let rb = c
        .request(m, url)
        .header("Depth", depth)
        .header("Content-Type", content_type)
        .body(body.into());
    let resp = apply_auth(rb, auth)
        .send()
        .await
        .map_err(explain_transport_error)?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() && status.as_u16() != 207 {
        return Err(format!("CardDAV {method} {url} -> {status}: {text}"));
    }
    Ok(text)
}

/// Fetch and parse all contacts from a CardDAV account. `base_url` may be the
/// addressbook collection itself or any DAV URL discovery can start from.
pub async fn sync_carddav(base_url: &str, auth: &DavAuth) -> Result<Vec<ParsedContact>, String> {
    let c = client()?;
    let contacts = sync_carddav_with_client(&c, base_url, auth).await?;
    Ok(contacts.into_iter().map(|card| card.contact).collect())
}

pub async fn sync_carddav_with_tls_options(
    base_url: &str,
    auth: &DavAuth,
    trusted_cert_der: Option<&[u8]>,
    accept_invalid_tls: bool,
) -> Result<Vec<ParsedContact>, String> {
    let c = client_with_tls_options(trusted_cert_der, accept_invalid_tls)?;
    let contacts = sync_carddav_with_client(&c, base_url, auth).await?;
    Ok(contacts.into_iter().map(|card| card.contact).collect())
}

pub async fn discover_carddav_addressbook(base_url: &str, auth: &DavAuth) -> Result<String, String> {
    let c = client()?;
    discover_addressbook(&c, base_url, auth)
        .await
        .ok_or_else(|| "no CardDAV addressbook discovered".to_string())
}

pub async fn carddav_sync_token(base_url: &str, auth: &DavAuth) -> Result<Option<String>, String> {
    let c = client()?;
    let addressbook = discover_addressbook(&c, base_url, auth)
        .await
        .unwrap_or_else(|| base_url.to_owned());
    collection_sync_token(&c, &addressbook, auth).await
}

pub async fn put_carddav_contact(
    href: &str,
    auth: &DavAuth,
    raw_vcard: &str,
) -> Result<(), String> {
    let c = client()?;
    dav(&c, "PUT", href, auth, "0", "text/vcard; charset=utf-8", raw_vcard).await?;
    Ok(())
}

pub async fn delete_carddav_contact(href: &str, auth: &DavAuth) -> Result<(), String> {
    let c = client()?;
    dav(&c, "DELETE", href, auth, "0", "text/plain", "").await?;
    Ok(())
}

async fn sync_carddav_with_client(
    c: &reqwest::Client,
    base_url: &str,
    auth: &DavAuth,
) -> Result<Vec<CardDavContact>, String> {
    let addressbook = discover_addressbook(c, base_url, auth)
        .await
        .unwrap_or_else(|| base_url.to_owned());

    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<C:addressbook-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:prop><D:getetag/><C:address-data/></D:prop>
</C:addressbook-query>"#;

    let xml = dav(
        c,
        "REPORT",
        &addressbook,
        auth,
        "1",
        "application/xml; charset=utf-8",
        body,
    )
    .await?;
    Ok(extract_carddav_contacts(&xml, &addressbook))
}

async fn collection_sync_token(
    c: &reqwest::Client,
    addressbook: &str,
    auth: &DavAuth,
) -> Result<Option<String>, String> {
    let xml = dav(
        c,
        "PROPFIND",
        addressbook,
        auth,
        "0",
        "application/xml; charset=utf-8",
        r#"<d:propfind xmlns:d="DAV:"><d:prop><d:sync-token/></d:prop></d:propfind>"#,
    )
    .await?;
    Ok(extract_texts(&xml, b"sync-token").into_iter().next())
}

async fn discover_addressbook(c: &reqwest::Client, base: &str, auth: &DavAuth) -> Option<String> {
    let principal_xml = dav(
        c,
        "PROPFIND",
        base,
        auth,
        "0",
        "application/xml; charset=utf-8",
        r#"<d:propfind xmlns:d="DAV:"><d:prop><d:current-user-principal/></d:prop></d:propfind>"#,
    )
    .await
    .ok()?;
    let principal = resolve(base, &first_href_in_elem(&principal_xml, b"current-user-principal")?)?;

    let home_xml = dav(
        c,
        "PROPFIND",
        &principal,
        auth,
        "0",
        "application/xml; charset=utf-8",
        r#"<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav"><d:prop><c:addressbook-home-set/></d:prop></d:propfind>"#,
    )
    .await
    .ok()?;
    let home = resolve(base, &first_href_in_elem(&home_xml, b"addressbook-home-set")?)?;

    let list_xml = dav(
        c,
        "PROPFIND",
        &home,
        auth,
        "1",
        "application/xml; charset=utf-8",
        r#"<d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/></d:prop></d:propfind>"#,
    )
    .await
    .ok()?;
    let href = first_href_with_resourcetype(&list_xml, b"addressbook")?;
    resolve(base, &href)
}

pub async fn graph_contacts(
    access_token: &str,
    delta_link: Option<&str>,
) -> Result<GraphSyncPage, String> {
    let c = client()?;
    let url = delta_link.unwrap_or("https://graph.microsoft.com/v1.0/me/contacts/delta");
    let json: Value = c
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(explain_transport_error)?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let contacts = json["value"]
        .as_array()
        .into_iter()
        .flatten()
        .map(graph_contact)
        .collect();
    Ok(GraphSyncPage {
        contacts,
        delta_link: json["@odata.deltaLink"].as_str().map(str::to_owned),
        next_link: json["@odata.nextLink"].as_str().map(str::to_owned),
    })
}

pub async fn graph_contact_folders(access_token: &str) -> Result<Vec<(String, String)>, String> {
    let c = client()?;
    let json: Value = c
        .get("https://graph.microsoft.com/v1.0/me/contactFolders")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(explain_transport_error)?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(json["value"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|folder| {
            Some((
                folder["id"].as_str()?.to_owned(),
                folder["displayName"].as_str()?.to_owned(),
            ))
        })
        .collect())
}

pub async fn graph_create_contact(
    access_token: &str,
    contact: &ParsedContact,
) -> Result<String, String> {
    let c = client()?;
    let json: Value = c
        .post("https://graph.microsoft.com/v1.0/me/contacts")
        .bearer_auth(access_token)
        .json(&to_graph_contact(contact))
        .send()
        .await
        .map_err(explain_transport_error)?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(json["id"].as_str().unwrap_or(&contact.uid).to_owned())
}

pub async fn graph_update_contact(
    access_token: &str,
    remote_id: &str,
    contact: &ParsedContact,
) -> Result<(), String> {
    let c = client()?;
    c.patch(format!("https://graph.microsoft.com/v1.0/me/contacts/{remote_id}"))
        .bearer_auth(access_token)
        .json(&to_graph_contact(contact))
        .send()
        .await
        .map_err(explain_transport_error)?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn graph_delete_contact(access_token: &str, remote_id: &str) -> Result<(), String> {
    let c = client()?;
    c.delete(format!("https://graph.microsoft.com/v1.0/me/contacts/{remote_id}"))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(explain_transport_error)?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn google_connections(
    access_token: &str,
    sync_token: Option<&str>,
) -> Result<GoogleSyncPage, String> {
    let c = client()?;
    let mut url = Url::parse("https://people.googleapis.com/v1/people/me/connections")
        .map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("personFields", "names,emailAddresses,phoneNumbers,addresses,organizations,biographies,photos")
        .append_pair("requestSyncToken", "true");
    if let Some(token) = sync_token {
        url.query_pairs_mut().append_pair("syncToken", token);
    }
    let json: Value = c
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(explain_transport_error)?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let contacts = json["connections"]
        .as_array()
        .into_iter()
        .flatten()
        .map(google_contact)
        .collect();
    Ok(GoogleSyncPage {
        contacts,
        sync_token: json["nextSyncToken"].as_str().map(str::to_owned),
        next_page_token: json["nextPageToken"].as_str().map(str::to_owned),
    })
}

pub async fn google_create_contact(
    access_token: &str,
    contact: &ParsedContact,
) -> Result<String, String> {
    let c = client()?;
    let json: Value = c
        .post("https://people.googleapis.com/v1/people:createContact")
        .bearer_auth(access_token)
        .json(&to_google_contact(contact))
        .send()
        .await
        .map_err(explain_transport_error)?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(json["resourceName"]
        .as_str()
        .unwrap_or(&contact.uid)
        .to_owned())
}

pub async fn google_update_contact(
    access_token: &str,
    resource_name: &str,
    contact: &ParsedContact,
) -> Result<(), String> {
    let c = client()?;
    c.patch(format!("https://people.googleapis.com/v1/{resource_name}:updateContact"))
        .bearer_auth(access_token)
        .query(&[("updatePersonFields", "names,emailAddresses,phoneNumbers,addresses,organizations,biographies")])
        .json(&to_google_contact(contact))
        .send()
        .await
        .map_err(explain_transport_error)?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn google_delete_contact(access_token: &str, resource_name: &str) -> Result<(), String> {
    let c = client()?;
    c.delete(format!("https://people.googleapis.com/v1/{resource_name}:deleteContact"))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(explain_transport_error)?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn parse_vcard(raw: &str) -> Option<ParsedContact> {
    if let Ok(mut cards) = vcard4::parse_loose(raw) {
        if let Some(card) = cards.pop() {
            let mut contact = parse_vcard_lines(raw);
            if contact.display_name.is_none() {
                contact.display_name = card.formatted_name.first().map(|p| p.value.clone());
            }
            if contact.given_name.is_none() || contact.family_name.is_none() {
                if let Some(name) = &card.name {
                    contact.family_name = contact.family_name.or_else(|| name.value.first().cloned());
                    contact.given_name = contact.given_name.or_else(|| name.value.get(1).cloned());
                }
            }
            if contact.emails.is_empty() {
                contact.emails = card
                    .email
                    .iter()
                    .map(|p| LabeledValue {
                        label: label_from_parameters(p.parameters.as_ref()),
                        value: p.value.clone(),
                    })
                    .collect();
            }
            if contact.phones.is_empty() {
                contact.phones = card
                    .tel
                    .iter()
                    .map(|p| LabeledValue {
                        label: label_from_parameters(p.parameters()),
                        value: p.to_string(),
                    })
                    .collect();
            }
            if contact.addresses.is_empty() {
                contact.addresses = card
                    .address
                    .iter()
                    .map(|p| PostalAddress {
                        label: label_from_parameters(p.parameters.as_ref()),
                        street: p.value.street_address.clone(),
                        locality: p.value.locality.clone(),
                        region: p.value.region.clone(),
                        postal_code: p.value.postal_code.clone(),
                        country: p.value.country_name.clone(),
                    })
                    .collect();
            }
            contact.org = contact.org.or_else(|| {
                card.org
                    .first()
                    .and_then(|p| p.value.iter().find(|v| !v.is_empty()).cloned())
            });
            contact.title = contact
                .title
                .or_else(|| card.title.first().map(|p| p.value.clone()));
            contact.notes = contact.notes.or_else(|| card.note.first().map(|p| p.value.clone()));
            contact.uid = contact.uid.trim().to_owned();
            if contact.uid.is_empty() {
                contact.uid = card
                    .uid
                    .clone()
                    .map(|uid| uid.to_string())
                    .filter(|uid| !uid.is_empty())
                    .unwrap_or_else(|| fallback_uid(&contact));
            }
            if contact.display_name.is_none() {
                contact.display_name = display_name_from_parts(&contact);
            }
            contact.raw_vcard = Some(raw.to_owned());
            return Some(contact);
        }
    }

    let mut contact = parse_vcard_lines(raw);
    if contact.uid.is_empty() {
        contact.uid = fallback_uid(&contact);
    }
    if contact.display_name.is_none() {
        contact.display_name = display_name_from_parts(&contact);
    }
    contact.raw_vcard = Some(raw.to_owned());
    contact.display_name.as_ref()?;
    Some(contact)
}

pub fn contact_to_vcard(contact: &ParsedContact) -> String {
    let uid = if contact.uid.is_empty() {
        fallback_uid(contact)
    } else {
        contact.uid.clone()
    };
    let display_name = contact
        .display_name
        .clone()
        .or_else(|| display_name_from_parts(contact))
        .unwrap_or_else(|| uid.clone());
    let family = contact.family_name.clone().unwrap_or_default();
    let given = contact.given_name.clone().unwrap_or_default();
    let mut out = vec![
        "BEGIN:VCARD".to_string(),
        "VERSION:4.0".to_string(),
        format!("UID:{}", escape_vcard_value(&uid)),
        format!("FN:{}", escape_vcard_value(&display_name)),
        format!("N:{};{};;;", escape_vcard_value(&family), escape_vcard_value(&given)),
    ];
    if let Some(org) = &contact.org {
        out.push(format!("ORG:{}", escape_vcard_value(org)));
    }
    if let Some(title) = &contact.title {
        out.push(format!("TITLE:{}", escape_vcard_value(title)));
    }
    for email in &contact.emails {
        out.push(format_labeled("EMAIL", email));
    }
    for phone in &contact.phones {
        out.push(format_labeled("TEL", phone));
    }
    for address in &contact.addresses {
        out.push(format_address(address));
    }
    if let Some(notes) = &contact.notes {
        out.push(format!("NOTE:{}", escape_vcard_value(notes)));
    }
    out.push("END:VCARD".to_string());
    out.join("\r\n")
}

fn graph_contact(value: &Value) -> ParsedContact {
    let emails = value["emailAddresses"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|email| {
            let address = email["address"].as_str()?;
            Some(LabeledValue {
                label: email["name"].as_str().map(str::to_owned),
                value: address.to_owned(),
            })
        })
        .collect();
    let phones = ["businessPhones", "homePhones"]
        .into_iter()
        .flat_map(|field| {
            value[field]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(move |phone| {
                    Some(LabeledValue {
                        label: Some(field.trim_end_matches("Phones").to_owned()),
                        value: phone.as_str()?.to_owned(),
                    })
                })
        })
        .collect();
    ParsedContact {
        uid: value["id"].as_str().unwrap_or_default().to_owned(),
        display_name: value["displayName"].as_str().map(str::to_owned),
        given_name: value["givenName"].as_str().map(str::to_owned),
        family_name: value["surname"].as_str().map(str::to_owned),
        org: value["companyName"].as_str().map(str::to_owned),
        title: value["jobTitle"].as_str().map(str::to_owned),
        emails,
        phones,
        addresses: Vec::new(),
        notes: value["personalNotes"].as_str().map(str::to_owned),
        photo_reference: None,
        raw_vcard: None,
    }
}

fn google_contact(value: &Value) -> ParsedContact {
    let name = value["names"].as_array().and_then(|names| names.first());
    let emails = value["emailAddresses"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|email| {
            Some(LabeledValue {
                label: email["type"].as_str().map(str::to_owned),
                value: email["value"].as_str()?.to_owned(),
            })
        })
        .collect();
    let phones = value["phoneNumbers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|phone| {
            Some(LabeledValue {
                label: phone["type"].as_str().map(str::to_owned),
                value: phone["value"].as_str()?.to_owned(),
            })
        })
        .collect();
    let addresses = value["addresses"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|address| PostalAddress {
            label: address["type"].as_str().map(str::to_owned),
            street: address["streetAddress"].as_str().map(str::to_owned),
            locality: address["city"].as_str().map(str::to_owned),
            region: address["region"].as_str().map(str::to_owned),
            postal_code: address["postalCode"].as_str().map(str::to_owned),
            country: address["country"].as_str().map(str::to_owned),
        })
        .collect();
    ParsedContact {
        uid: value["resourceName"].as_str().unwrap_or_default().to_owned(),
        display_name: name.and_then(|n| n["displayName"].as_str()).map(str::to_owned),
        given_name: name.and_then(|n| n["givenName"].as_str()).map(str::to_owned),
        family_name: name.and_then(|n| n["familyName"].as_str()).map(str::to_owned),
        org: value["organizations"]
            .as_array()
            .and_then(|orgs| orgs.first())
            .and_then(|org| org["name"].as_str())
            .map(str::to_owned),
        title: value["organizations"]
            .as_array()
            .and_then(|orgs| orgs.first())
            .and_then(|org| org["title"].as_str())
            .map(str::to_owned),
        emails,
        phones,
        addresses,
        notes: value["biographies"]
            .as_array()
            .and_then(|notes| notes.first())
            .and_then(|note| note["value"].as_str())
            .map(str::to_owned),
        photo_reference: value["photos"]
            .as_array()
            .and_then(|photos| photos.first())
            .and_then(|photo| photo["url"].as_str())
            .map(str::to_owned),
        raw_vcard: None,
    }
}

fn to_graph_contact(contact: &ParsedContact) -> Value {
    serde_json::json!({
        "displayName": contact.display_name,
        "givenName": contact.given_name,
        "surname": contact.family_name,
        "companyName": contact.org,
        "jobTitle": contact.title,
        "personalNotes": contact.notes,
        "emailAddresses": contact.emails.iter().map(|email| serde_json::json!({
            "name": email.label,
            "address": email.value,
        })).collect::<Vec<_>>(),
        "businessPhones": contact.phones.iter().filter(|p| p.label.as_deref() == Some("work")).map(|p| p.value.clone()).collect::<Vec<_>>(),
        "homePhones": contact.phones.iter().filter(|p| p.label.as_deref() == Some("home")).map(|p| p.value.clone()).collect::<Vec<_>>(),
    })
}

fn to_google_contact(contact: &ParsedContact) -> Value {
    serde_json::json!({
        "names": [{
            "displayName": contact.display_name,
            "givenName": contact.given_name,
            "familyName": contact.family_name,
        }],
        "emailAddresses": contact.emails.iter().map(|email| serde_json::json!({
            "type": email.label,
            "value": email.value,
        })).collect::<Vec<_>>(),
        "phoneNumbers": contact.phones.iter().map(|phone| serde_json::json!({
            "type": phone.label,
            "value": phone.value,
        })).collect::<Vec<_>>(),
        "addresses": contact.addresses.iter().map(|address| serde_json::json!({
            "type": address.label,
            "streetAddress": address.street,
            "city": address.locality,
            "region": address.region,
            "postalCode": address.postal_code,
            "country": address.country,
        })).collect::<Vec<_>>(),
        "organizations": [{
            "name": contact.org,
            "title": contact.title,
        }],
        "biographies": contact.notes.as_ref().map(|note| vec![serde_json::json!({ "value": note })]).unwrap_or_default(),
    })
}

fn parse_vcard_lines(raw: &str) -> ParsedContact {
    let unfolded = unfold(raw);
    let mut contact = ParsedContact::default();
    for line in unfolded.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let mut key_parts = key.split(';');
        let prop = key_parts
            .next()
            .unwrap_or("")
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        let label = key_parts.find_map(label_from_param_text);
        let value = unescape_vcard_value(value.trim());
        if value.is_empty() {
            continue;
        }
        match prop.as_str() {
            "UID" => contact.uid = value,
            "FN" => contact.display_name = Some(value),
            "N" => {
                let mut parts = value.split(';');
                contact.family_name = optional_string(parts.next().unwrap_or_default());
                contact.given_name = optional_string(parts.next().unwrap_or_default());
            }
            "EMAIL" => contact.emails.push(LabeledValue { label, value }),
            "TEL" => contact.phones.push(LabeledValue { label, value }),
            "ADR" => {
                let parts: Vec<&str> = value.split(';').collect();
                contact.addresses.push(PostalAddress {
                    label,
                    street: parts.get(2).and_then(|v| optional_string(v)),
                    locality: parts.get(3).and_then(|v| optional_string(v)),
                    region: parts.get(4).and_then(|v| optional_string(v)),
                    postal_code: parts.get(5).and_then(|v| optional_string(v)),
                    country: parts.get(6).and_then(|v| optional_string(v)),
                });
            }
            "ORG" => contact.org = optional_string(value.split(';').next().unwrap_or(&value)),
            "TITLE" => contact.title = Some(value),
            "NOTE" => contact.notes = Some(value),
            "PHOTO" => contact.photo_reference = Some(value),
            _ => {}
        }
    }
    contact
}

fn extract_carddav_contacts(xml: &str, base: &str) -> Vec<CardDavContact> {
    let mut reader = Reader::from_str(xml);
    let mut in_response = false;
    let mut in_href = false;
    let mut in_etag = false;
    let mut in_address_data = false;
    let mut href = String::new();
    let mut etag = String::new();
    let mut card = String::new();
    let mut out = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let ln = local_name(name.as_ref());
                match ln {
                    b"response" => {
                        in_response = true;
                        href.clear();
                        etag.clear();
                        card.clear();
                    }
                    b"href" if in_response && href.is_empty() => in_href = true,
                    b"getetag" if in_response => in_etag = true,
                    b"address-data" if in_response => in_address_data = true,
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if let Ok(text) = e.unescape() {
                    if in_href {
                        href.push_str(&text);
                    } else if in_etag {
                        etag.push_str(&text);
                    } else if in_address_data {
                        card.push_str(&text);
                    }
                }
            }
            Ok(Event::CData(e)) if in_address_data => {
                card.push_str(&String::from_utf8_lossy(&e.into_inner()));
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let ln = local_name(name.as_ref());
                match ln {
                    b"href" => in_href = false,
                    b"getetag" => in_etag = false,
                    b"address-data" => in_address_data = false,
                    b"response" => {
                        in_response = false;
                        if let Some(contact) = parse_vcard(&card) {
                            out.push(CardDavContact {
                                href: resolve(base, href.trim()).unwrap_or_else(|| href.clone()),
                                etag: optional_string(etag.trim()),
                                contact,
                            });
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

fn local_name(qname: &[u8]) -> &[u8] {
    match qname.iter().rposition(|&b| b == b':') {
        Some(i) => &qname[i + 1..],
        None => qname,
    }
}

fn extract_texts(xml: &str, target: &[u8]) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut buf = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if local_name(e.name().as_ref()) == target {
                    depth += 1;
                    if depth == 1 {
                        buf.clear();
                    }
                }
            }
            Ok(Event::Text(e)) if depth > 0 => {
                if let Ok(t) = e.unescape() {
                    buf.push_str(&t);
                }
            }
            Ok(Event::CData(e)) if depth > 0 => {
                buf.push_str(&String::from_utf8_lossy(&e.into_inner()));
            }
            Ok(Event::End(e)) => {
                if local_name(e.name().as_ref()) == target && depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        out.push(std::mem::take(&mut buf));
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

fn first_href_in_elem(xml: &str, target: &[u8]) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut in_target = 0i32;
    let mut in_href = false;
    let mut buf = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let ln = local_name(name.as_ref());
                if ln == target {
                    in_target += 1;
                } else if in_target > 0 && ln == b"href" {
                    in_href = true;
                    buf.clear();
                }
            }
            Ok(Event::Text(e)) if in_href => {
                if let Ok(t) = e.unescape() {
                    buf.push_str(&t);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let ln = local_name(name.as_ref());
                if ln == b"href" && in_href {
                    return Some(buf.trim().to_owned());
                }
                if ln == target && in_target > 0 {
                    in_target -= 1;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    None
}

fn first_href_with_resourcetype(xml: &str, rtype: &[u8]) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let (mut in_response, mut in_href, mut in_rtype) = (false, false, 0i32);
    let mut matched = false;
    let mut href: Option<String> = None;
    let mut buf = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let ln = local_name(name.as_ref());
                if ln == b"response" {
                    in_response = true;
                    matched = false;
                    href = None;
                } else if in_response && ln == b"href" && href.is_none() {
                    in_href = true;
                    buf.clear();
                } else if ln == b"resourcetype" {
                    in_rtype += 1;
                } else if in_rtype > 0 && ln == rtype {
                    matched = true;
                }
            }
            Ok(Event::Empty(e)) => {
                if in_rtype > 0 && local_name(e.name().as_ref()) == rtype {
                    matched = true;
                }
            }
            Ok(Event::Text(e)) if in_href => {
                if let Ok(t) = e.unescape() {
                    buf.push_str(&t);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let ln = local_name(name.as_ref());
                if ln == b"href" && in_href {
                    href = Some(buf.trim().to_owned());
                    in_href = false;
                } else if ln == b"resourcetype" && in_rtype > 0 {
                    in_rtype -= 1;
                } else if ln == b"response" {
                    in_response = false;
                    if matched {
                        if let Some(h) = href.take() {
                            return Some(h);
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    None
}

fn resolve(base: &str, href: &str) -> Option<String> {
    Url::parse(base)
        .ok()?
        .join(href)
        .ok()
        .map(|u| u.to_string())
}

fn label_from_parameters(
    parameters: Option<&vcard4::parameter::Parameters>,
) -> Option<String> {
    parameters?
        .types
        .as_ref()?
        .iter()
        .map(ToString::to_string)
        .find(|value| !value.is_empty())
}

fn label_from_param_text(param: &str) -> Option<String> {
    let (name, value) = param.split_once('=')?;
    if !name.eq_ignore_ascii_case("TYPE") {
        return None;
    }
    value
        .split(',')
        .next()
        .map(|v| v.trim_matches('"').to_ascii_lowercase())
        .filter(|v| !v.is_empty())
}

fn optional_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn display_name_from_parts(contact: &ParsedContact) -> Option<String> {
    let name = [contact.given_name.as_deref(), contact.family_name.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    optional_string(&name).or_else(|| contact.emails.first().map(|email| email.value.clone()))
}

fn fallback_uid(contact: &ParsedContact) -> String {
    let seed = contact
        .emails
        .first()
        .map(|email| email.value.as_str())
        .or(contact.display_name.as_deref())
        .unwrap_or("contact");
    format!("mailquill-{}", seed.replace(['@', ' ', '<', '>'], "-"))
}

fn format_labeled(property: &str, value: &LabeledValue) -> String {
    let label = value
        .label
        .as_deref()
        .filter(|label| !label.is_empty())
        .map(|label| format!(";TYPE={}", label.replace([';', ':', ','], "")))
        .unwrap_or_default();
    format!("{property}{label}:{}", escape_vcard_value(&value.value))
}

fn format_address(address: &PostalAddress) -> String {
    let label = address
        .label
        .as_deref()
        .filter(|label| !label.is_empty())
        .map(|label| format!(";TYPE={}", label.replace([';', ':', ','], "")))
        .unwrap_or_default();
    format!(
        "ADR{label}:;;{};{};{};{};{}",
        escape_vcard_value(address.street.as_deref().unwrap_or_default()),
        escape_vcard_value(address.locality.as_deref().unwrap_or_default()),
        escape_vcard_value(address.region.as_deref().unwrap_or_default()),
        escape_vcard_value(address.postal_code.as_deref().unwrap_or_default()),
        escape_vcard_value(address.country.as_deref().unwrap_or_default())
    )
}

fn escape_vcard_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace(';', "\\;")
        .replace(',', "\\,")
}

fn unescape_vcard_value(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

/// Join RFC 5322 folded continuation lines (leading space/tab).
fn unfold(raw: &str) -> String {
    let mut out = String::new();
    for line in raw.replace("\r\n", "\n").split('\n') {
        if line.starts_with(' ') || line.starts_with('\t') {
            out.push_str(line.trim_start());
        } else {
            out.push('\n');
            out.push_str(line);
        }
    }
    out
}
