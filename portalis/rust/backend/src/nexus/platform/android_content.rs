//! Android's native gateway to MediaStore and Storage Access Framework.
//!
//! Rust owns this gateway because torrent piece I/O runs in Rust. Flutter may
//! request a URI in the future, but it must never relay file bytes or turn a
//! URI into a cache path. The installed application context will be used by
//! the Android `ContentLocation` storage adapter.

use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    os::fd::FromRawFd,
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::Context as _;
use jni::EnvUnowned;
use jni::objects::{Global, JClass, JObject, JValue};
use jni::vm::JavaVM;

static JVM: OnceLock<JavaVM> = OnceLock::new();
static APPLICATION_CONTEXT: OnceLock<Global<JObject<'static>>> = OnceLock::new();

const MAX_OPEN_DOCUMENT_DESCRIPTORS: usize = 64;

#[derive(Default)]
struct DescriptorCache {
    files: HashMap<String, Arc<File>>,
    order: VecDeque<String>,
}

static DESCRIPTORS: OnceLock<Mutex<DescriptorCache>> = OnceLock::new();

/// One persisted Android document, with the descriptor reused for its entire
/// hashing/seeding lifetime. Reopening through JNI for every torrent block is
/// both expensive and hostile to remote document providers.
#[derive(Debug)]
pub(crate) struct Source {
    uri: String,
}

impl Source {
    pub(crate) fn new(uri: &str) -> Self {
        Self {
            uri: uri.to_owned(),
        }
    }

    pub(crate) fn length(&self, known_length: Option<u64>) -> anyhow::Result<u64> {
        self.with_file(|file| {
            let measured = file.metadata()?.len();
            Ok(if measured == 0 {
                known_length.unwrap_or(0)
            } else {
                measured
            })
        })
    }

    pub(crate) fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> anyhow::Result<()> {
        use std::os::unix::fs::FileExt;
        self.with_file(|file| {
            file.read_exact_at(buffer, offset)?;
            Ok(())
        })
    }

    fn with_file<T>(
        &self,
        operation: impl FnOnce(&File) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let file = cached_file(&self.uri)?;
        operation(&file)
    }
}

fn cached_file(uri: &str) -> anyhow::Result<Arc<File>> {
    let cache = DESCRIPTORS.get_or_init(|| Mutex::new(DescriptorCache::default()));
    {
        let mut cache = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(file) = cache.files.get(uri).cloned() {
            cache.order.retain(|key| key != uri);
            cache.order.push_back(uri.to_owned());
            return Ok(file);
        }
    }

    // Do not hold the global cache lock across a provider call. A slow or
    // remote DocumentsProvider must not stall unrelated source reads.
    let opened = Arc::new(open(uri)?);
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(file) = cache.files.get(uri).cloned() {
        cache.order.retain(|key| key != uri);
        cache.order.push_back(uri.to_owned());
        return Ok(file);
    }
    cache.files.insert(uri.to_owned(), Arc::clone(&opened));
    cache.order.push_back(uri.to_owned());
    while cache.files.len() > MAX_OPEN_DOCUMENT_DESCRIPTORS {
        let Some(oldest) = cache.order.pop_front() else {
            break;
        };
        cache.files.remove(&oldest);
    }
    crate::nexus::log::clog!("content", "opened Android content source");
    Ok(opened)
}

/// Called exactly once by `PortalisNative.install` during Android activity
/// startup. Keeping the application context, not an Activity, prevents a
/// rotation from invalidating long-lived torrent I/O.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_portalis_PortalisNative_installContext(
    mut env: EnvUnowned<'_>,
    _class: JClass,
    context: JObject,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let vm = env.get_java_vm()?;
        let context = env.new_global_ref(context)?;
        let _ = JVM.set(vm);
        let _ = APPLICATION_CONTEXT.set(context);
        Ok(())
    })
    .resolve::<jni::errors::LogErrorAndDefault>();
}

/// Opens a fresh descriptor for a persisted Storage Access Framework URI.
///
/// A descriptor belongs to one piece read and is closed with its Rust
/// [`File`]. We never retain an Activity, copy the provider's media into a
/// cache path, or send its bytes through Flutter.
pub(crate) fn open(uri: &str) -> anyhow::Result<File> {
    let vm = JVM
        .get()
        .context("Android content access is unavailable before the app context is installed")?;
    let context = APPLICATION_CONTEXT
        .get()
        .context("Android content access has no application context")?;
    vm.attach_current_thread(|env| -> anyhow::Result<File> {
        let raw_uri = JObject::from(env.new_string(uri)?);
        let parsed_uri = env
            .call_static_method(
                jni::jni_str!("android/net/Uri"),
                jni::jni_str!("parse"),
                jni::jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
                &[JValue::Object(&raw_uri)],
            )?
            .l()?;
        let resolver = env
            .call_method(
                context,
                jni::jni_str!("getContentResolver"),
                jni::jni_sig!("()Landroid/content/ContentResolver;"),
                &[],
            )?
            .l()?;
        let mode = JObject::from(env.new_string("r")?);
        let descriptor = env
            .call_method(
                resolver,
                jni::jni_str!("openFileDescriptor"),
                jni::jni_sig!(
                    "(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;"
                ),
                &[JValue::Object(&parsed_uri), JValue::Object(&mode)],
            )?
            .l()?;
        anyhow::ensure!(
            !descriptor.is_null(),
            "Android Files could not open the selected document"
        );
        let fd = env
            .call_method(
                descriptor,
                jni::jni_str!("detachFd"),
                jni::jni_sig!("()I"),
                &[],
            )?
            .i()?;
        anyhow::ensure!(fd >= 0, "Android Files returned an invalid descriptor");
        // `detachFd` transfers ownership to us, so this File is responsible
        // for closing it even when a piece read fails.
        Ok(unsafe { File::from_raw_fd(fd) })
    })
    .map_err(|error| anyhow::anyhow!("opening Android content source: {error}"))
}
