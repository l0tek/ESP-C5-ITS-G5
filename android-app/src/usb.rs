//! Android USB Host API via JNI. No root, network bridge or Java plugin required.
use crate::{runtime::Runtime, stream::LineDecoder};
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JObject, JString, JValue},
};
use std::{
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Session {
    vm: Arc<JavaVM>,
    connection: GlobalRef,
    claimed: Vec<GlobalRef>,
}
impl Drop for Session {
    fn drop(&mut self) {
        if let Ok(mut env) = self.vm.attach_current_thread() {
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_clear();
            }
            for interface in self.claimed.iter().rev() {
                let _ = env.call_method(
                    self.connection.as_obj(),
                    "releaseInterface",
                    "(Landroid/hardware/usb/UsbInterface;)Z",
                    &[JValue::Object(interface.as_obj())],
                );
            }
            let _ = env.call_method(self.connection.as_obj(), "close", "()V", &[]);
        }
    }
}
pub fn receive(runtime: &Runtime) -> std::result::Result<(), String> {
    let ctx = ndk_context::android_context();
    // Tao owns the VM/context and keeps them valid for the Android activity lifecycle.
    let vm = Arc::new(unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?);
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    let activity = env.new_global_ref(activity).map_err(|e| e.to_string())?;
    let result = env.with_local_frame(64, |env| {
        receive_inner(env, activity.as_obj(), vm.clone(), runtime)
    });
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    result.map_err(|e: Box<dyn std::error::Error>| e.to_string())
}
fn receive_inner(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
    vm: Arc<JavaVM>,
    runtime: &Runtime,
) -> Result<()> {
    let service = env.new_string("usb")?;
    let manager = env
        .call_method(
            activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&service)],
        )?
        .l()?;
    let devices = env
        .call_method(&manager, "getDeviceList", "()Ljava/util/HashMap;", &[])?
        .l()?;
    let values = env
        .call_method(&devices, "values", "()Ljava/util/Collection;", &[])?
        .l()?;
    let iter = env
        .call_method(values, "iterator", "()Ljava/util/Iterator;", &[])?
        .l()?;
    let mut matches = Vec::new();
    while env.call_method(&iter, "hasNext", "()Z", &[])?.z()? {
        let device = env
            .call_method(&iter, "next", "()Ljava/lang/Object;", &[])?
            .l()?;
        let vendor = env.call_method(&device, "getVendorId", "()I", &[])?.i()?;
        let product = env.call_method(&device, "getProductId", "()I", &[])?.i()?;
        if vendor == 0x303a && product == 0x1001 {
            matches.push(env.new_global_ref(device)?);
        }
    }
    if matches.len() != 1 {
        return Err("Genau einen ESP32-C5 (303a:1001) über USB-OTG anschließen".into());
    }
    let device = matches.remove(0);
    let name_obj = JString::from(
        env.call_method(
            device.as_obj(),
            "getDeviceName",
            "()Ljava/lang/String;",
            &[],
        )?
        .l()?,
    );
    let name: String = env.get_string(&name_obj)?.into();
    let permitted = |env: &mut JNIEnv<'_>| -> Result<bool> {
        Ok(env
            .call_method(
                &manager,
                "hasPermission",
                "(Landroid/hardware/usb/UsbDevice;)Z",
                &[JValue::Object(device.as_obj())],
            )?
            .z()?)
    };
    if !permitted(env)? {
        runtime.data.lock().unwrap().status = "USB-Zugriff im Android-Dialog erlauben…".into();
        let action = env.new_string("org.l0tek.citsmap.USB_PERMISSION")?;
        let intent = env.new_object(
            "android/content/Intent",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&action)],
        )?;
        let package = env
            .call_method(activity, "getPackageName", "()Ljava/lang/String;", &[])?
            .l()?;
        env.call_method(
            &intent,
            "setPackage",
            "(Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&package)],
        )?;
        let pending = env
            .call_static_method(
                "android/app/PendingIntent",
                "getBroadcast",
                "(Landroid/content/Context;ILandroid/content/Intent;I)Landroid/app/PendingIntent;",
                &[
                    JValue::Object(activity),
                    JValue::Int(0),
                    JValue::Object(&intent),
                    JValue::Int(0x04000000 | 0x08000000),
                ],
            )?
            .l()?;
        env.call_method(
            &manager,
            "requestPermission",
            "(Landroid/hardware/usb/UsbDevice;Landroid/app/PendingIntent;)V",
            &[JValue::Object(device.as_obj()), JValue::Object(&pending)],
        )?;
        let until = Instant::now() + Duration::from_secs(30);
        while !permitted(env)? {
            if runtime.cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            if Instant::now() >= until {
                return Err(
                    "USB-Zugriff nicht erteilt. Erneut verbinden und Zugriff erlauben.".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    let count = env
        .call_method(device.as_obj(), "getInterfaceCount", "()I", &[])?
        .i()?;
    let (mut control, mut data, mut endpoint) = (None, None, None);
    for i in 0..count {
        let interface = env
            .call_method(
                device.as_obj(),
                "getInterface",
                "(I)Landroid/hardware/usb/UsbInterface;",
                &[JValue::Int(i)],
            )?
            .l()?;
        let class = env
            .call_method(&interface, "getInterfaceClass", "()I", &[])?
            .i()?;
        if class == 2 {
            control = Some(env.new_global_ref(&interface)?);
        }
        if class != 10 {
            continue;
        }
        let n = env
            .call_method(&interface, "getEndpointCount", "()I", &[])?
            .i()?;
        for j in 0..n {
            let ep = env
                .call_method(
                    &interface,
                    "getEndpoint",
                    "(I)Landroid/hardware/usb/UsbEndpoint;",
                    &[JValue::Int(j)],
                )?
                .l()?;
            let kind = env.call_method(&ep, "getType", "()I", &[])?.i()?;
            let direction = env.call_method(&ep, "getDirection", "()I", &[])?.i()?;
            if kind == 2 && direction == 128 {
                data = Some(env.new_global_ref(&interface)?);
                endpoint = Some(env.new_global_ref(ep)?);
            }
        }
    }
    let data = data.ok_or("CDC-Dateninterface fehlt")?;
    let endpoint = endpoint.ok_or("USB-Bulk-IN-Endpunkt fehlt")?;
    let connection = env
        .call_method(
            &manager,
            "openDevice",
            "(Landroid/hardware/usb/UsbDevice;)Landroid/hardware/usb/UsbDeviceConnection;",
            &[JValue::Object(device.as_obj())],
        )?
        .l()?;
    if connection.is_null() {
        return Err("USB-Gerät konnte nicht geöffnet werden".into());
    }
    let mut session = Session {
        vm,
        connection: env.new_global_ref(connection)?,
        claimed: Vec::new(),
    };
    for interface in control.iter().chain(std::iter::once(&data)) {
        if !env
            .call_method(
                session.connection.as_obj(),
                "claimInterface",
                "(Landroid/hardware/usb/UsbInterface;Z)Z",
                &[JValue::Object(interface.as_obj()), JValue::Bool(1)],
            )?
            .z()?
        {
            return Err("USB-Interface wird bereits verwendet".into());
        }
        session.claimed.push(interface.clone());
    }
    if let Some(control) = control {
        let id = env
            .call_method(control.as_obj(), "getId", "()I", &[])?
            .i()?;
        let result = env
            .call_method(
                session.connection.as_obj(),
                "controlTransfer",
                "(IIII[BII)I",
                &[
                    JValue::Int(0x21),
                    JValue::Int(0x22),
                    JValue::Int(1),
                    JValue::Int(id),
                    JValue::Object(&JObject::null()),
                    JValue::Int(0),
                    JValue::Int(1000),
                ],
            )?
            .i()?;
        if result < 0 {
            return Err("CDC-DTR konnte nicht gesetzt werden".into());
        }
    }
    runtime.data.lock().unwrap().status = "USB verbunden · warte auf Pakete".into();
    let mut lines = LineDecoder::default();
    let mut checked = Instant::now();
    while !runtime.cancel.load(Ordering::Relaxed) {
        env.with_local_frame(16, |env| -> Result<()> {
            let buffer = env.new_byte_array(4096)?;
            let n = env
                .call_method(
                    session.connection.as_obj(),
                    "bulkTransfer",
                    "(Landroid/hardware/usb/UsbEndpoint;[BII)I",
                    &[
                        JValue::Object(endpoint.as_obj()),
                        JValue::Object(&buffer),
                        JValue::Int(4096),
                        JValue::Int(250),
                    ],
                )?
                .i()?;
            if n > 0 {
                let bytes = env.convert_byte_array(&buffer)?;
                runtime.feed(&mut lines, &bytes[..n as usize]);
            }
            if checked.elapsed() >= Duration::from_secs(2) {
                let devices = env
                    .call_method(&manager, "getDeviceList", "()Ljava/util/HashMap;", &[])?
                    .l()?;
                let key = env.new_string(&name)?;
                if !env
                    .call_method(
                        devices,
                        "containsKey",
                        "(Ljava/lang/Object;)Z",
                        &[JValue::Object(&key)],
                    )?
                    .z()?
                {
                    return Err("ESP32-C5 wurde abgezogen".into());
                }
                checked = Instant::now();
            }
            Ok(())
        })?;
    }
    Ok(())
}
