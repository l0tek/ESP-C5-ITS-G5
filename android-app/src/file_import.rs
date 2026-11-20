//! Android's document picker, independent of HTML file-input handling.
#[cfg(target_os = "android")]
pub async fn pick_capture() -> Result<Option<(Vec<u8>, String)>, String> {
    use jni::objects::{JByteArray, JObject, JString};
    enum Selection {
        Pending,
        Cancelled,
        Ready(Vec<u8>, String),
    }
    let context = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) }.map_err(|e| e.to_string())?;
    let call = |start: bool| -> Result<Selection, String> {
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let activity = unsafe { JObject::from_raw(context.context().cast()) };
        let result =
            env.with_local_frame(16, |env| -> Result<Selection, Box<dyn std::error::Error>> {
                if start {
                    env.call_method(&activity, "startCitsImport", "()V", &[])?;
                    return Ok(Selection::Pending);
                }
                let status = env
                    .call_method(&activity, "getCitsImportStatus", "()I", &[])?
                    .i()?;
                match status {
                    0 => Ok(Selection::Pending),
                    2 => Ok(Selection::Cancelled),
                    1 => {
                        let name = JString::from(
                            env.call_method(
                                &activity,
                                "getCitsImportName",
                                "()Ljava/lang/String;",
                                &[],
                            )?
                            .l()?,
                        );
                        let name: String = env.get_string(&name)?.into();
                        let array = JByteArray::from(
                            env.call_method(&activity, "takeCitsImportBytes", "()[B", &[])?
                                .l()?,
                        );
                        if array.is_null() {
                            return Err("Keine Dateidaten erhalten".into());
                        }
                        if env.get_array_length(&array)? as usize > crate::stream::MAX_IMPORT {
                            return Err("Datei größer als 8 MiB".into());
                        }
                        Ok(Selection::Ready(env.convert_byte_array(array)?, name))
                    }
                    3 => {
                        let error = JString::from(
                            env.call_method(
                                &activity,
                                "getCitsImportError",
                                "()Ljava/lang/String;",
                                &[],
                            )?
                            .l()?,
                        );
                        let error: String = env.get_string(&error)?.into();
                        Err(error.into())
                    }
                    _ => Err("Dateiauswahl wurde beendet; bitte erneut öffnen".into()),
                }
            });
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        result.map_err(|e| e.to_string())
    };
    call(true)?;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        match call(false)? {
            Selection::Pending => {}
            Selection::Cancelled => return Ok(None),
            Selection::Ready(bytes, name) => return Ok(Some((bytes, name))),
        }
    }
}

#[cfg(not(target_os = "android"))]
pub async fn pick_capture() -> Result<Option<(Vec<u8>, String)>, String> {
    Err("Native Dateiauswahl ist nur unter Android verfügbar".into())
}
