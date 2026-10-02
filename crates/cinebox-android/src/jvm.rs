//! The process's JVM: one-time setup the Rust side cannot do on its own, and
//! calls into `CineboxActivity`.

use std::sync::OnceLock;

use jni::objects::{JObject, JValue};
use jni::refs::Global;
use jni::{Env, JavaVM, jni_sig, jni_str};
use winit::platform::android::activity::AndroidApp;

/// Outlives every activity instance, so FFmpeg and the TLS verifier keep it.
static APP_CONTEXT: OnceLock<Global<JObject<'static>>> = OnceLock::new();

/// Released when collected, so it is kept for the process lifetime.
static MULTICAST_LOCK: OnceLock<Global<JObject<'static>>> = OnceLock::new();

pub struct Java {
    vm: JavaVM,
    activity: Global<JObject<'static>>,
}

impl Java {
    /// Run `call` with the activity on the current thread; a failure is logged.
    pub fn with_activity<T, F>(&self, what: &str, call: F) -> Option<T>
    where
        F: FnOnce(&mut Env, &JObject) -> jni::errors::Result<T>,
    {
        let result = self
            .vm
            .attach_current_thread(|env| call(env, self.activity.as_obj()));

        match result {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(%error, what, "call into Android failed");
                None
            }
        }
    }
}

pub fn init(app: &AndroidApp) -> Option<Java> {
    // SAFETY: android-activity hands out the process's JavaVM pointer.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let activity = app.activity_as_ptr() as jni::sys::jobject;

    let result = vm.attach_current_thread(|env| -> jni::errors::Result<_> {
        // SAFETY: an unowned global reference that stays valid while `app` lives;
        // `JObject` does not delete it on drop.
        let activity = unsafe { JObject::from_raw(env, activity) };
        init_with_activity(env, &activity, &vm)?;
        env.new_global_ref(&activity)
    });

    match result {
        Ok(activity) => Some(Java { vm, activity }),
        Err(error) => {
            tracing::error!(%error, "Android JNI setup failed");
            None
        }
    }
}

fn init_with_activity(env: &mut Env, activity: &JObject, vm: &JavaVM) -> jni::errors::Result<()> {
    let context = env
        .call_method(
            activity,
            jni_str!("getApplicationContext"),
            jni_sig!("()Landroid/content/Context;"),
            &[],
        )?
        .l()?;
    let global = env.new_global_ref(&context)?;
    let context = APP_CONTEXT.get_or_init(|| global);

    // reqwest and hickory verify TLS through Android's trust store.
    let local = env.new_local_ref(context.as_obj())?;
    rustls_platform_verifier::android::init_with_env(env, local)?;

    // SAFETY: both are valid for the process lifetime (`APP_CONTEXT` is never dropped).
    unsafe {
        cinebox_player::init_mediacodec(vm.get_raw().cast(), context.as_raw().cast());
    }

    if let Err(error) = acquire_multicast_lock(env, context) {
        tracing::warn!(%error, "no multicast lock; TorrServer discovery may find nothing");
    }

    Ok(())
}

/// Wi-Fi drivers drop multicast unless an app holds this lock, and mDNS
/// discovery of TorrServer depends on it.
fn acquire_multicast_lock(env: &mut Env, context: &JObject) -> jni::errors::Result<()> {
    let service = env.new_string("wifi")?;
    let wifi = env
        .call_method(
            context,
            jni_str!("getSystemService"),
            jni_sig!("(Ljava/lang/String;)Ljava/lang/Object;"),
            &[JValue::Object(&service)],
        )?
        .l()?;

    let tag = env.new_string("cinebox-mdns")?;
    let lock = env
        .call_method(
            &wifi,
            jni_str!("createMulticastLock"),
            jni_sig!("(Ljava/lang/String;)Landroid/net/wifi/WifiManager$MulticastLock;"),
            &[JValue::Object(&tag)],
        )?
        .l()?;

    env.call_method(
        &lock,
        jni_str!("setReferenceCounted"),
        jni_sig!("(Z)V"),
        &[JValue::Bool(false)],
    )?
    .v()?;

    env.call_method(&lock, jni_str!("acquire"), jni_sig!("()V"), &[])?
        .v()?;

    let _ = MULTICAST_LOCK.set(env.new_global_ref(&lock)?);
    Ok(())
}
