//! FFmpeg's MediaCodec decoders call into Java, so they need the process's
//! JavaVM and the app context before the first `loadfile`.

use std::ffi::{c_int, c_void};

#[link(name = "avcodec")]
unsafe extern "C" {
    fn av_jni_set_java_vm(vm: *mut c_void, log_ctx: *mut c_void) -> c_int;
    fn av_jni_set_android_app_ctx(app_ctx: *mut c_void, log_ctx: *mut c_void) -> c_int;
}

/// Hand FFmpeg the JavaVM and a global reference to the Android context.
///
/// # Safety
///
/// `vm` must be the process's `JavaVM*` and `context` a JNI global reference
/// to an `android.content.Context`; both must stay valid for the process lifetime.
pub unsafe fn init_mediacodec(vm: *mut c_void, context: *mut c_void) {
    // SAFETY: the caller guarantees both pointers; FFmpeg only stores them.
    let vm_status = unsafe { av_jni_set_java_vm(vm, std::ptr::null_mut()) };
    if vm_status < 0 {
        tracing::warn!(status = vm_status, "FFmpeg refused the JavaVM; MediaCodec is off");
        return;
    }

    // SAFETY: as above.
    let ctx_status = unsafe { av_jni_set_android_app_ctx(context, std::ptr::null_mut()) };
    if ctx_status < 0 {
        tracing::warn!(status = ctx_status, "FFmpeg refused the Android context");
    }
}
