//! Model transports.

pub mod foundation;
pub mod root;
pub mod server;
pub(crate) mod wire;

pub use foundation::{
    FoundationModelProvider, LocalModelAvailability, PreparedFoundationTransport,
};
pub use root::{PreparedRootTransport, RootModelProvider};
pub use server::{PreparedServerTransport, ServerModelProvider};

#[cfg(test)]
fn resize_test_instructions(
    prompt: &mut floe_agent_contract::prompts::PromptAssembly,
    bytes: usize,
) {
    let first_bytes = (bytes - 6).min(4096);
    let remaining = bytes - first_bytes - 4;
    let second_bytes = (remaining - 1).min(4096);
    let sizes = [first_bytes, second_bytes, remaining - second_bytes];
    assert_eq!(prompt.components.len(), sizes.len());
    for (component, size) in prompt.components.iter_mut().zip(sizes) {
        assert!(size <= 4096);
        component.content = "가".repeat(size / 3) + &"x".repeat(size % 3);
    }
    assert_eq!(prompt.render().len(), bytes);
}
