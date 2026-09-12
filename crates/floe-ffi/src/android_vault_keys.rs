#![cfg(target_os = "android")]

use std::sync::OnceLock;

use floe_agent::AgentFailure;
use floe_core::{VaultKey, VaultKeyProvider};
use floe_domain::PersonId;
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JByteArray, JObject, JValue},
    sys::jboolean,
};
use uuid::Uuid;
use zeroize::Zeroizing;

struct AndroidVaultKeyState {
    vm: JavaVM,
    store: GlobalRef,
}

static STATE: OnceLock<AndroidVaultKeyState> = OnceLock::new();

#[derive(Clone, Copy, Default)]
pub(crate) struct AndroidVaultKeys;

impl VaultKeyProvider for AndroidVaultKeys {
    fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<VaultKey, AgentFailure> {
        let bytes = call_load(person_id, vault_id)?;
        if bytes.len() != 32 {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut key = [0; 32];
        key.copy_from_slice(&bytes);
        Ok(VaultKey::from_bytes(key))
    }

    fn insert(
        &self,
        person_id: PersonId,
        vault_id: Uuid,
        key: &VaultKey,
    ) -> Result<(), AgentFailure> {
        let state = STATE.get().ok_or(AgentFailure::VaultUnavailable)?;
        let mut env = state
            .vm
            .attach_current_thread()
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let person = env
            .new_string(person_id.to_string())
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let vault = env
            .new_string(vault_id.to_string())
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let bytes = env
            .byte_array_from_slice(key.as_bytes())
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let result = env.call_method(
            state.store.as_obj(),
            "insert",
            "(Ljava/lang/String;Ljava/lang/String;[B)Z",
            &[
                JValue::Object(person.as_ref()),
                JValue::Object(vault.as_ref()),
                JValue::Object(bytes.as_ref()),
            ],
        );
        let _ = env.set_byte_array_region(&bytes, 0, &[0; 32]);
        let result = result.map_err(|_| AgentFailure::VaultUnavailable)?;
        if result.z().map_err(|_| AgentFailure::VaultUnavailable)? {
            Ok(())
        } else {
            Err(AgentFailure::VaultUnavailable)
        }
    }
}

fn call_load(person_id: PersonId, vault_id: Uuid) -> Result<Zeroizing<Vec<u8>>, AgentFailure> {
    let state = STATE.get().ok_or(AgentFailure::VaultUnavailable)?;
    let mut env = state
        .vm
        .attach_current_thread()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let person = env
        .new_string(person_id.to_string())
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let vault = env
        .new_string(vault_id.to_string())
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let result = env
        .call_method(
            state.store.as_obj(),
            "load",
            "(Ljava/lang/String;Ljava/lang/String;)[B",
            &[
                JValue::Object(person.as_ref()),
                JValue::Object(vault.as_ref()),
            ],
        )
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let object = result.l().map_err(|_| AgentFailure::VaultUnavailable)?;
    if object.is_null() {
        return Err(AgentFailure::VaultUnavailable);
    }
    let bytes = JByteArray::from(object);
    if env
        .get_array_length(&bytes)
        .map_err(|_| AgentFailure::VaultUnavailable)?
        != 32
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let result = env
        .convert_byte_array(&bytes)
        .map(Zeroizing::new)
        .map_err(|_| AgentFailure::VaultUnavailable);
    let _ = env.set_byte_array_region(&bytes, 0, &[0; 32]);
    result
}

pub(crate) fn initialize(mut env: JNIEnv<'_>, activity: JObject<'_>) -> bool {
    if STATE.get().is_some() {
        return true;
    }
    let vm = match env.get_java_vm() {
        Ok(vm) => vm,
        Err(_) => return false,
    };
    let context = match env
        .call_method(
            &activity,
            "getApplicationContext",
            "()Landroid/content/Context;",
            &[],
        )
        .and_then(|value| value.l())
    {
        Ok(context) if !context.is_null() => context,
        _ => return false,
    };
    let class = match env.find_class("app/floe/floe_client/AndroidVaultKeyStore") {
        Ok(class) => class,
        Err(_) => return false,
    };
    let store = match env.new_object(
        class,
        "(Landroid/content/Context;)V",
        &[JValue::Object(&context)],
    ) {
        Ok(store) => store,
        Err(_) => return false,
    };
    let store = match env.new_global_ref(store) {
        Ok(store) => store,
        Err(_) => return false,
    };
    STATE.set(AndroidVaultKeyState { vm, store }).is_ok() || STATE.get().is_some()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_floe_floe_1client_MainActivity_nativeConfigureVault(
    env: JNIEnv<'_>,
    activity: JObject<'_>,
) -> jboolean {
    if initialize(env, activity) { 1 } else { 0 }
}
