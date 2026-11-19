#[cfg(target_os = "android")]
pub fn save_pcap(bytes: &[u8], timestamp_ms: u64) -> Result<String, String> {
    use jni::objects::{JByteArray, JObject, JValue};

    let context = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    let result = env.with_local_frame(32, |env| -> Result<String, Box<dyn std::error::Error>> {
        let sdk = env
            .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
            .i()?;
        if sdk < 29 {
            return Err("PCAP-Export benötigt Android 10 oder neuer".into());
        }
        let values = env.new_object("android/content/ContentValues", "()V", &[])?;
        let put = |env: &mut jni::JNIEnv<'_>, key: &str, value: &str| -> jni::errors::Result<()> {
            let key = env.new_string(key)?;
            let value = env.new_string(value)?;
            env.call_method(
                &values,
                "put",
                "(Ljava/lang/String;Ljava/lang/String;)V",
                &[JValue::Object(&key), JValue::Object(&value)],
            )?;
            Ok(())
        };
        let filename = format!("cits-{timestamp_ms}.pcap");
        put(env, "_display_name", &filename)?;
        put(env, "mime_type", "application/vnd.tcpdump.pcap")?;
        put(env, "relative_path", "Download/C-ITS Map")?;
        let resolver = env
            .call_method(
                &activity,
                "getContentResolver",
                "()Landroid/content/ContentResolver;",
                &[],
            )?
            .l()?;
        let collection = env
            .get_static_field(
                "android/provider/MediaStore$Downloads",
                "EXTERNAL_CONTENT_URI",
                "Landroid/net/Uri;",
            )?
            .l()?;
        let uri = env
            .call_method(
                &resolver,
                "insert",
                "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;",
                &[JValue::Object(&collection), JValue::Object(&values)],
            )?
            .l()?;
        if uri.is_null() {
            return Err("Android konnte keine Exportdatei anlegen".into());
        }
        let stream = env
            .call_method(
                &resolver,
                "openOutputStream",
                "(Landroid/net/Uri;)Ljava/io/OutputStream;",
                &[JValue::Object(&uri)],
            )?
            .l()?;
        let array: JByteArray = env.byte_array_from_slice(bytes)?;
        let write_result = env.call_method(&stream, "write", "([B)V", &[JValue::Object(&array)]);
        let close_result = env.call_method(&stream, "close", "()V", &[]);
        if let Err(error) = write_result {
            let _ = env.call_method(
                &resolver,
                "delete",
                "(Landroid/net/Uri;Ljava/lang/String;[Ljava/lang/String;)I",
                &[
                    JValue::Object(&uri),
                    JValue::Object(&JObject::null()),
                    JValue::Object(&JObject::null()),
                ],
            );
            return Err(error.into());
        }
        close_result?;
        Ok(format!(
            "{filename} wurde unter Downloads/C-ITS Map gespeichert"
        ))
    });
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    result.map_err(|e| e.to_string())
}

#[cfg(not(target_os = "android"))]
pub fn save_pcap(bytes: &[u8], timestamp_ms: u64) -> Result<String, String> {
    let filename = format!("cits-{timestamp_ms}.pcap");
    std::fs::write(&filename, bytes).map_err(|e| e.to_string())?;
    Ok(format!("{filename} wurde gespeichert"))
}
