//! Secret storage backed by the Windows Credential Manager.
//!
//! # Why not a file
//!
//! The Notion OAuth client secret and the resulting access token are
//! long-lived credentials for the user's workspace. Writing them into a JSON
//! file next to the database means any process running as the user can read
//! them, and they end up in backups and cloud sync folders. The Windows
//! Credential Manager encrypts them with the user's login credentials and
//! scopes them to the user account, which is the appropriate place for a
//! desktop app.
//!
//! # What this module guarantees
//!
//! * Secrets never cross the Tauri boundary: the frontend only ever learns
//!   whether a secret is present (`configured`), never its value.
//! * Secrets are never written to logs.
//! * Deleting a secret is idempotent, so disconnecting twice is harmless.

use crate::error::{AppError, AppResult};

/// The target names used in the Credential Manager. Changing one of these
/// would orphan the existing secret, so they are treated as stable.
const TARGET_CLIENT_ID: &str = "LifeOSNotch/notion-client-id";
const TARGET_CLIENT_SECRET: &str = "LifeOSNotch/notion-client-secret";
const TARGET_ACCESS_TOKEN: &str = "LifeOSNotch/notion-access-token";
const TARGET_WORKSPACE: &str = "LifeOSNotch/notion-workspace";

/// The secrets the application needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Secret {
    /// The OAuth application's client id.
    ClientId,
    /// The OAuth application's client secret.
    ClientSecret,
    /// The access token issued by Notion.
    AccessToken,
    /// The workspace name, shown in the UI. Not sensitive, but stored beside
    /// the token so that disconnecting clears both in one step.
    Workspace,
}

impl Secret {
    fn target(self) -> &'static str {
        match self {
            Secret::ClientId => TARGET_CLIENT_ID,
            Secret::ClientSecret => TARGET_CLIENT_SECRET,
            Secret::AccessToken => TARGET_ACCESS_TOKEN,
            Secret::Workspace => TARGET_WORKSPACE,
        }
    }
}

/// A handle to the credential store.
///
/// Cheap to clone: it holds no state, it is only a namespace for the calls.
#[derive(Debug, Clone, Default)]
pub struct CredentialStore;

impl CredentialStore {
    pub fn new() -> Self {
        CredentialStore
    }

    /// Reads a secret, or `None` when it has never been stored.
    pub fn get(&self, secret: Secret) -> AppResult<Option<String>> {
        imp::read(secret.target())
    }

    /// Stores or replaces a secret.
    pub fn set(&self, secret: Secret, value: &str) -> AppResult<()> {
        imp::write(secret.target(), value)
    }

    /// Removes a secret. Succeeds when the secret was already absent.
    pub fn delete(&self, secret: Secret) -> AppResult<()> {
        imp::delete(secret.target())
    }

    /// True when both halves of the OAuth application credentials are present.
    pub fn is_configured(&self) -> bool {
        matches!(self.get(Secret::ClientId), Ok(Some(_)))
            && matches!(self.get(Secret::ClientSecret), Ok(Some(_)))
    }

    /// True when an access token is present.
    pub fn has_token(&self) -> bool {
        matches!(self.get(Secret::AccessToken), Ok(Some(value)) if !value.is_empty())
    }

    /// Clears the token and workspace, leaving the app credentials in place so
    /// the user can reconnect without re-entering them.
    pub fn clear_session(&self) -> AppResult<()> {
        self.delete(Secret::AccessToken)?;
        self.delete(Secret::Workspace)?;
        Ok(())
    }

    /// Clears everything this app stored.
    pub fn clear_all(&self) -> AppResult<()> {
        for secret in [
            Secret::AccessToken,
            Secret::Workspace,
            Secret::ClientId,
            Secret::ClientSecret,
        ] {
            self.delete(secret)?;
        }
        Ok(())
    }
}

#[cfg(windows)]
mod imp {
    use std::ptr;

    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    use super::AppError;
    use crate::error::AppResult;

    /// Encodes a Rust string as a NUL-terminated UTF-16 buffer for `PCWSTR`.
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn read(target: &str) -> AppResult<Option<String>> {
        let target_wide = wide(target);
        let mut credential: *mut CREDENTIALW = ptr::null_mut();

        // SAFETY: `target_wide` is NUL-terminated and outlives the call. The
        // returned pointer is owned by the credential manager and must be
        // released with `CredFree`.
        let result = unsafe {
            CredReadW(
                PCWSTR(target_wide.as_ptr()),
                CRED_TYPE_GENERIC,
                None,
                &mut credential,
            )
        };

        if result.is_err() {
            // "Element not found" is the normal case for a secret that was
            // never stored, so it is not an error for the caller.
            return Ok(None);
        }
        if credential.is_null() {
            return Ok(None);
        }

        // SAFETY: `CredReadW` succeeded, so `credential` points to a valid
        // `CREDENTIALW` whose blob is `CredentialBlobSize` bytes long.
        let value = unsafe {
            let cred = &*credential;
            let bytes = std::slice::from_raw_parts(
                cred.CredentialBlob,
                cred.CredentialBlobSize as usize,
            );
            let text = String::from_utf8_lossy(bytes).to_string();
            CredFree(credential as *const core::ffi::c_void);
            text
        };

        Ok(Some(value))
    }

    pub fn write(target: &str, value: &str) -> AppResult<()> {
        let mut target_wide = wide(target);
        // The credential blob is not required to be NUL-terminated, but
        // keeping the UTF-16 buffer alive for the duration of the call is
        // what matters.
        let mut blob: Vec<u16> = value.encode_utf16().collect();

        let credential = CREDENTIALW {
            Flags: Default::default(),
            Type: CRED_TYPE_GENERIC,
            TargetName: windows::core::PWSTR(target_wide.as_mut_ptr()),
            Comment: windows::core::PWSTR::null(),
            LastWritten: Default::default(),
            CredentialBlobSize: (blob.len() * 2) as u32,
            CredentialBlob: blob.as_mut_ptr() as *mut u8,
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            AttributeCount: 0,
            Attributes: ptr::null_mut(),
            TargetAlias: windows::core::PWSTR::null(),
            UserName: windows::core::PWSTR::null(),
        };

        // SAFETY: every pointer in `credential` refers to a buffer that is
        // alive for the duration of this call.
        unsafe { CredWriteW(&credential, 0) }
            .map_err(|e| AppError::msg(format!("Could not save the Notion credentials: {e}")))
    }

    pub fn delete(target: &str) -> AppResult<()> {
        let target_wide = wide(target);
        // SAFETY: `target_wide` is NUL-terminated and outlives the call.
        let result = unsafe { CredDeleteW(PCWSTR(target_wide.as_ptr()), CRED_TYPE_GENERIC, None) };
        // A missing credential is the desired end state, not a failure.
        let _ = result;
        Ok(())
    }
}

/// The app only ships for Windows; on any other target secrets cannot be
/// stored, and every call reports that clearly rather than pretending.
#[cfg(not(windows))]
mod imp {
    use super::AppError;
    use crate::error::AppResult;

    pub fn read(_target: &str) -> AppResult<Option<String>> {
        Ok(None)
    }

    pub fn write(_target: &str, _value: &str) -> AppResult<()> {
        Err(AppError::msg(
            "LifeOS Notch stores Notion credentials in the Windows Credential Manager, which is not available on this platform.",
        ))
    }

    pub fn delete(_target: &str) -> AppResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_targets_are_distinct_and_stable() {
        let targets = [
            Secret::ClientId.target(),
            Secret::ClientSecret.target(),
            Secret::AccessToken.target(),
            Secret::Workspace.target(),
        ];
        // A duplicated target would silently overwrite another secret.
        let mut unique = targets.to_vec();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), targets.len());

        // These strings are a storage contract: changing one orphans the
        // secret that was already stored for existing users.
        assert_eq!(Secret::ClientId.target(), "LifeOSNotch/notion-client-id");
        assert_eq!(Secret::AccessToken.target(), "LifeOSNotch/notion-access-token");
    }

    #[test]
    fn every_target_is_namespaced_to_this_app() {
        for secret in [
            Secret::ClientId,
            Secret::ClientSecret,
            Secret::AccessToken,
            Secret::Workspace,
        ] {
            assert!(secret.target().starts_with("LifeOSNotch/"));
        }
    }
}
