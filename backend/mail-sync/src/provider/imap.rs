//! IMAP implementation of [`MailProvider`] — a thin adapter over the existing
//! session functions, tracking the currently selected folder so repeated calls
//! against the same folder skip the SELECT round-trip.

use async_trait::async_trait;

use super::{FolderStatus, MailProvider, ProviderConfig, ProviderError};
use crate::session::{self, FetchedMessage, FolderInfo, ImapSession};

pub struct ImapProvider {
    session: ImapSession,
    selected: Option<String>,
}

impl ImapProvider {
    pub async fn connect(config: &ProviderConfig) -> Result<Self, ProviderError> {
        let session = session::connect_imap(
            &config.host,
            config.port,
            &config.username,
            &config.password,
            config.oauth_access_token.as_deref(),
            &config.auth_scheme,
            config.trusted_cert_der.as_deref(),
        )
        .await?;
        Ok(Self {
            session,
            selected: None,
        })
    }

    pub(crate) async fn ensure_selected(&mut self, folder: &str) -> Result<(), ProviderError> {
        if self.selected.as_deref() != Some(folder) {
            session::select_folder(&mut self.session, folder).await?;
            self.selected = Some(folder.to_owned());
        }
        Ok(())
    }

    /// Underlying IMAP session — lets the Gmail-over-IMAP hybrid issue the extra
    /// `X-GM-MSGID` fetch on the same connection after a normal fetch selected
    /// the folder.
    pub(crate) fn session_mut(&mut self) -> &mut ImapSession {
        &mut self.session
    }
}

#[async_trait]
impl MailProvider for ImapProvider {
    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>, ProviderError> {
        Ok(session::list_folders(&mut self.session).await?)
    }

    async fn folder_status(&mut self, folder: &str) -> Result<FolderStatus, ProviderError> {
        // Always re-SELECT: UIDVALIDITY/EXISTS must reflect the server's
        // current state, not a cached selection.
        let (uidvalidity, exists) = session::select_folder(&mut self.session, folder).await?;
        self.selected = Some(folder.to_owned());
        Ok(FolderStatus {
            uidvalidity,
            exists,
        })
    }

    async fn highest_uid(&mut self, folder: &str) -> Result<Option<u32>, ProviderError> {
        self.ensure_selected(folder).await?;
        Ok(session::highest_uid(&mut self.session).await?)
    }

    async fn fetch_flags(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<(u32, bool, bool, bool)>, ProviderError> {
        self.ensure_selected(folder).await?;
        Ok(session::fetch_flags(&mut self.session, uid_set).await?)
    }

    async fn fetch_headers(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<FetchedMessage>, ProviderError> {
        self.ensure_selected(folder).await?;
        Ok(session::fetch_headers(&mut self.session, uid_set).await?)
    }

    async fn fetch_full(
        &mut self,
        folder: &str,
        uid_set: &str,
    ) -> Result<Vec<FetchedMessage>, ProviderError> {
        self.ensure_selected(folder).await?;
        Ok(session::fetch_full(&mut self.session, uid_set).await?)
    }

    async fn fetch_raw(&mut self, folder: &str, uid: u32) -> Result<Vec<u8>, ProviderError> {
        self.ensure_selected(folder).await?;
        Ok(session::fetch_body_uid(&mut self.session, uid).await?)
    }

    async fn set_flag(
        &mut self,
        folder: &str,
        uid: u32,
        flag: &str,
        value: bool,
    ) -> Result<(), ProviderError> {
        self.ensure_selected(folder).await?;
        Ok(session::set_flag(&mut self.session, uid, flag, value).await?)
    }

    async fn move_message(
        &mut self,
        src_folder: &str,
        uid: u32,
        dest_folder: &str,
    ) -> Result<(), ProviderError> {
        self.ensure_selected(src_folder).await?;
        Ok(session::move_message(&mut self.session, uid, dest_folder, false).await?)
    }

    async fn delete_permanently(&mut self, folder: &str, uid: u32) -> Result<(), ProviderError> {
        self.ensure_selected(folder).await?;
        Ok(session::expunge_uid(&mut self.session, uid).await?)
    }

    async fn append_sent(&mut self, raw_message: &[u8]) -> Result<(), ProviderError> {
        Ok(session::append_to_sent(&mut self.session, raw_message).await?)
    }

    async fn close(&mut self) -> Result<(), ProviderError> {
        let _ = self.session.logout().await;
        self.selected = None;
        Ok(())
    }
}
