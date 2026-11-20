//! Android BLE central via the native BluetoothGatt API exposed by MainActivity.
use crate::{runtime::Runtime, stream::LineDecoder};
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JByteArray, JObject, JString},
};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

struct Session {
    vm: Arc<JavaVM>,
    activity: GlobalRef,
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Ok(mut env) = self.vm.attach_current_thread() {
            let _ = env.call_method(self.activity.as_obj(), "stopCitsBle", "()V", &[]);
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_clear();
            }
        }
    }
}

pub fn receive(runtime: &Runtime) -> Result<(), String> {
    let ctx = ndk_context::android_context();
    let vm = Arc::new(unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?);
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    let activity = env.new_global_ref(activity).map_err(|e| e.to_string())?;
    let session = Session {
        vm: vm.clone(),
        activity,
    };
    env.call_method(session.activity.as_obj(), "startCitsBle", "()V", &[])
        .map_err(|e| e.to_string())?;
    let mut lines = LineDecoder::default();
    let mut announced = false;
    loop {
        if runtime.cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let status = env
            .call_method(session.activity.as_obj(), "getCitsBleStatus", "()I", &[])
            .and_then(|v| v.i())
            .map_err(|e| e.to_string())?;
        match status {
            0 => {}
            1 => {
                if !announced {
                    runtime.data.lock().unwrap().status =
                        "Bluetooth verbunden · warte auf Pakete".into();
                    announced = true;
                }
                loop {
                    let bytes = env
                        .call_method(session.activity.as_obj(), "takeCitsBleBytes", "()[B", &[])
                        .and_then(|v| v.l())
                        .map_err(|e| e.to_string())?;
                    if bytes.is_null() {
                        break;
                    }
                    let bytes = env
                        .convert_byte_array(&JByteArray::from(bytes))
                        .map_err(|e| e.to_string())?;
                    runtime.feed(&mut lines, &bytes);
                }
            }
            2 => return Ok(()),
            3 => {
                let error = env
                    .call_method(
                        session.activity.as_obj(),
                        "getCitsBleError",
                        "()Ljava/lang/String;",
                        &[],
                    )
                    .and_then(|v| v.l())
                    .map_err(|e| e.to_string())?;
                let error: String = env
                    .get_string(&JString::from(error))
                    .map_err(|e| e.to_string())?
                    .into();
                return Err(error);
            }
            _ => return Err("Unbekannter Bluetooth-Status".into()),
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn set_external_antenna(external: bool) -> Result<(), String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    let queued = env
        .call_method(
            &activity,
            "setCitsExternalAntenna",
            "(Z)Z",
            &[jni::objects::JValue::Bool(external.into())],
        )
        .and_then(|value| value.z())
        .map_err(|e| e.to_string())?;
    if queued {
        Ok(())
    } else {
        Err("Antennenbefehl konnte nicht gesendet werden".into())
    }
}
