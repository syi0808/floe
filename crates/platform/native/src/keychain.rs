//! Exact OS generic-password slots. Queries never show authentication UI or hide
//! locked items as absence. Secret records are device-only and nonsynchronizing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeychainError {
    Unavailable,
    Locked,
    Ambiguous,
    TooLarge,
}
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple {
    use super::KeychainError;
    use core_foundation::{
        array::CFArray,
        base::{CFType, TCFType},
        boolean::CFBoolean,
        data::CFData,
        dictionary::CFDictionary,
        string::{CFString, CFStringRef},
    };
    use security_framework_sys::{
        access_control::kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
        item::*,
        keychain_item::{SecItemAdd, SecItemCopyMatching, SecItemDelete, SecItemUpdate},
    };
    // security-framework-sys 2.17 does not declare these SecItem.h constants.
    // Bind the actual Security symbols; UISkip would conceal locked items.
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        #[link_name = "kSecUseAuthenticationUIFail"]
        static AUTHENTICATION_UI_FAIL: CFStringRef;
        #[link_name = "kSecAttrAccessible"]
        static ATTR_ACCESSIBLE: CFStringRef;
    }
    fn key(value: CFStringRef) -> CFString {
        unsafe { CFString::wrap_under_get_rule(value) }
    }
    fn text(value: CFStringRef) -> CFType {
        key(value).into_CFType()
    }
    fn query(service: &str, account: &str) -> Vec<(CFString, CFType)> {
        unsafe {
            vec![
                (key(kSecClass), text(kSecClassGenericPassword)),
                (key(kSecAttrService), CFString::new(service).into_CFType()),
                (key(kSecAttrAccount), CFString::new(account).into_CFType()),
                (
                    key(kSecAttrSynchronizable),
                    CFBoolean::false_value().into_CFType(),
                ),
                // macOS enforces ThisDeviceOnly accessibility in this keychain.
                #[cfg(target_os = "macos")]
                (
                    key(kSecUseDataProtectionKeychain),
                    CFBoolean::true_value().into_CFType(),
                ),
                (key(kSecUseAuthenticationUI), text(AUTHENTICATION_UI_FAIL)),
            ]
        }
    }
    fn error(operation: &'static str, code: i32) -> KeychainError {
        let failure = match code {
            -25308 | -25293 => KeychainError::Locked,
            _ => KeychainError::Unavailable,
        };
        #[cfg(debug_assertions)]
        tracing::warn!(
            operation,
            os_status = code,
            kind = match failure {
                KeychainError::Locked => "locked",
                KeychainError::Unavailable => "unavailable",
                KeychainError::Ambiguous => "ambiguous",
                KeychainError::TooLarge => "too_large",
            },
            "native_keychain_failure"
        );
        #[cfg(not(debug_assertions))]
        let _ = operation;
        failure
    }
    pub fn read(
        service: &str,
        account: &str,
        max: usize,
    ) -> Result<Option<Vec<u8>>, KeychainError> {
        let mut fields = query(service, account);
        unsafe {
            fields.push((key(kSecReturnData), CFBoolean::true_value().into_CFType()));
            fields.push((key(kSecMatchLimit), text(kSecMatchLimitAll)));
        }
        let query = CFDictionary::from_CFType_pairs(&fields);
        let mut output = std::ptr::null();
        let code = unsafe { SecItemCopyMatching(query.as_concrete_TypeRef(), &mut output) };
        if code == -25300 {
            return Ok(None);
        }
        if code != 0 {
            return Err(error("copy_matching", code));
        }
        if output.is_null() {
            return Err(KeychainError::Unavailable);
        }
        let result = unsafe { CFType::wrap_under_create_rule(output) };
        let array = result
            .downcast::<CFArray>()
            .ok_or(KeychainError::Ambiguous)?;
        if array.len() != 1 {
            return Err(KeychainError::Ambiguous);
        }
        let value = unsafe {
            CFType::wrap_under_get_rule(*array.get(0).ok_or(KeychainError::Unavailable)?)
        };
        let data = value
            .downcast::<CFData>()
            .ok_or(KeychainError::Unavailable)?;
        if data.len() as usize > max {
            return Err(KeychainError::TooLarge);
        }
        Ok(Some(data.bytes().to_vec()))
    }
    pub fn replace(service: &str, account: &str, bytes: &[u8]) -> Result<(), KeychainError> {
        let fields = query(service, account);
        let mut attributes = vec![
            (
                unsafe { key(kSecValueData) },
                CFData::from_buffer(bytes).into_CFType(),
            ),
            (unsafe { key(ATTR_ACCESSIBLE) }, unsafe {
                text(kSecAttrAccessibleWhenUnlockedThisDeviceOnly)
            }),
        ];
        let query = CFDictionary::from_CFType_pairs(&fields);
        let update = CFDictionary::from_CFType_pairs(&attributes);
        let status =
            unsafe { SecItemUpdate(query.as_concrete_TypeRef(), update.as_concrete_TypeRef()) };
        if status == 0 {
            return Ok(());
        }
        if status != -25300 {
            return Err(error("update", status));
        }
        attributes.extend(fields);
        let add = CFDictionary::from_CFType_pairs(&attributes);
        let status = unsafe { SecItemAdd(add.as_concrete_TypeRef(), std::ptr::null_mut()) };
        if status == 0 {
            Ok(())
        } else {
            Err(error("add", status))
        }
    }
    pub fn delete(service: &str, account: &str) -> Result<(), KeychainError> {
        let query = CFDictionary::from_CFType_pairs(&query(service, account));
        let status = unsafe { SecItemDelete(query.as_concrete_TypeRef()) };
        if status == 0 || status == -25300 {
            Ok(())
        } else {
            Err(error("delete", status))
        }
    }
}
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use apple::{
    delete as delete_generic_password, read as read_generic_password,
    replace as write_generic_password,
};
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub fn read_generic_password(_: &str, _: &str, _: usize) -> Result<Option<Vec<u8>>, KeychainError> {
    Err(KeychainError::Unavailable)
}
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub fn write_generic_password(_: &str, _: &str, _: &[u8]) -> Result<(), KeychainError> {
    Err(KeychainError::Unavailable)
}
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub fn delete_generic_password(_: &str, _: &str) -> Result<(), KeychainError> {
    Err(KeychainError::Unavailable)
}
