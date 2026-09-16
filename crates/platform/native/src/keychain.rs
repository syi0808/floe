//! Keychain item access. The caller owns what the bytes mean.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeychainError {
    /// The key slot could not be read.
    Unavailable,
    /// More than one item matched the exact slot.
    Ambiguous,
    /// The stored secret exceeded the caller's bound.
    TooLarge,
}

/// Read one generic password for an exact service/account slot.
///
/// Returns `Ok(None)` when the slot has never been written. A slot that matches
/// more than one item is an error, never a silent first-match.
#[cfg(target_os = "macos")]
pub fn read_generic_password(
    service: &str,
    account: &str,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, KeychainError> {
    use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
    use security_framework_sys::base::errSecItemNotFound;

    let mut query = ItemSearchOptions::new();
    query
        .class(ItemClass::generic_password())
        .service(service)
        .account(account)
        .load_data(true)
        .skip_authenticated_items(true);
    let results = match query.search() {
        Ok(results) => results,
        Err(error) if error.code() == errSecItemNotFound => return Ok(None),
        Err(_) => return Err(KeychainError::Unavailable),
    };
    let mut results = results.into_iter();
    let secret = match (results.next(), results.next()) {
        (Some(SearchResult::Data(secret)), None) => secret,
        _ => return Err(KeychainError::Ambiguous),
    };
    if secret.len() > max_bytes {
        return Err(KeychainError::TooLarge);
    }
    Ok(Some(secret))
}

#[cfg(not(target_os = "macos"))]
pub fn read_generic_password(
    _service: &str,
    _account: &str,
    _max_bytes: usize,
) -> Result<Option<Vec<u8>>, KeychainError> {
    Ok(None)
}
