use jni::objects::{GlobalRef, JClass, JObject, JObjectArray, JString, JValue};
use jni::JNIEnv;
use std::io;
use std::sync::OnceLock;
use std::time::Duration;

struct BluetoothBridge {
    vm: jni::JavaVM,
    class: GlobalRef,
}

static BRIDGE: OnceLock<Result<BluetoothBridge, String>> = OnceLock::new();

pub fn initialize(env: &mut JNIEnv, activity: &JObject) {
    BRIDGE.get_or_init(|| {
        let initialize = || -> jni::errors::Result<BluetoothBridge> {
            let loader = env.call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?.l()?;
            let name = env.new_string("com.obdium.app.ElmBluetooth")?;
            let class = env.call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&name)])?.l()?;
            Ok(BluetoothBridge { vm: env.get_java_vm()?, class: env.new_global_ref(class)? })
        };
        let mut initialize = initialize;
        initialize().map_err(|error| error.to_string())
    });
}

fn with_class<T>(
    body: impl FnOnce(&mut JNIEnv, &JClass) -> jni::errors::Result<T>,
) -> Result<T, String> {
    let bridge = BRIDGE.get().ok_or("Bluetooth is still initializing. Please retry.")?
        .as_ref().map_err(Clone::clone)?;
    let mut env = bridge.vm.attach_current_thread().map_err(|error| error.to_string())?;
    let class: &JClass = bridge.class.as_obj().into();
    env.with_local_frame(32, |env| {
        let result = body(env, class);
        if matches!(result, Err(jni::errors::Error::JavaException)) {
            env.exception_clear()?;
        }
        result
    }).map_err(|error| error.to_string())
}

pub fn paired_device_names() -> Result<Vec<String>, String> {
    let response = with_class(|env, class| {
        let value = env.call_static_method(class, "bondedNames", "()[Ljava/lang/String;", &[])?;
        let array = JObjectArray::from(value.l()?);
        let length = env.get_array_length(&array)?;
        let mut names = Vec::with_capacity(length as usize);
        for index in 0..length {
            let item = JString::from(env.get_object_array_element(&array, index)?);
            let text = env.get_string(&item)?;
            names.push(text.to_string_lossy().into_owned());
        }
        Ok(names)
    })?;
    let mut fields = response.into_iter();
    match fields.next().as_deref() {
        Some("OK") => Ok(fields.collect()),
        Some(status) => Err(status.to_owned()),
        None => Err("BLUETOOTH_UNAVAILABLE".to_owned()),
    }
}

pub fn bonded_names() -> Vec<String> {
    paired_device_names().unwrap_or_default()
}

pub fn connect(target: &str) -> Result<(), String> {
    let target = target.to_string();
    let joined = std::thread::spawn(move || {
        with_class(|env, class| {
            let name = env.new_string(&target)?;
            let value = env.call_static_method(
                class,
                "connect",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&name)],
            )?;
            let result = JString::from(value.l()?);
            let text = env.get_string(&result)?;
            Ok(text.to_string_lossy().into_owned())
        })
    })
    .join()
    .map_err(|_| "bluetooth connect thread failed".to_string())?;

    match joined {
        Ok(status) if status == "OK" => Ok(()),
        Ok(status) => Err(status),
        Err(error) => Err(error),
    }
}

pub fn write(data: &[u8]) -> io::Result<()> {
    with_class(|env, class| {
        let bytes = env.byte_array_from_slice(data)?;
        env.call_static_method(class, "write", "([B)V", &[JValue::Object(&bytes)])?;
        Ok(())
    })
    .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error))
}

pub fn read(max: usize, timeout: Duration) -> io::Result<Vec<u8>> {
    let timeout_ms = timeout.as_millis().clamp(20, 10_000) as i32;
    let bytes = with_class(|env, class| {
        let value = env.call_static_method(
            class,
            "read",
            "(II)[B",
            &[JValue::Int(max as i32), JValue::Int(timeout_ms)],
        )?;
        let array = jni::objects::JByteArray::from(value.l()?);
        env.convert_byte_array(&array)
    })
    .map_err(|error| io::Error::new(io::ErrorKind::BrokenPipe, error))?;

    if bytes.is_empty() {
        Err(io::Error::new(io::ErrorKind::TimedOut, "elm timeout"))
    } else {
        Ok(bytes)
    }
}

pub fn clear_input() {
    let _ = with_class(|env, class| {
        env.call_static_method(class, "clearInput", "()V", &[])?;
        Ok(())
    });
}

pub fn close() {
    let _ = with_class(|env, class| {
        env.call_static_method(class, "close", "()V", &[])?;
        Ok(())
    });
}
