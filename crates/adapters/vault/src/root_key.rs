//! Secret key bytes shared by physically separate encrypted stores.
use floe_kernel::AgentFailure;
use zeroize::Zeroizing;
pub struct RootKey(Zeroizing<[u8; 32]>);
impl RootKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    pub(crate) fn generate() -> Result<Self, AgentFailure> {
        let mut key = Self::from_bytes([0; 32]);
        getrandom::fill(key.0.as_mut()).map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok(key)
    }
    pub(crate) fn hex(&self) -> String {
        const DIGITS: &[u8] = b"0123456789abcdef";
        let mut output = String::with_capacity(64);
        for byte in self.as_bytes() {
            output.push(DIGITS[(byte >> 4) as usize] as char);
            output.push(DIGITS[(byte & 15) as usize] as char);
        }
        output
    }
}
