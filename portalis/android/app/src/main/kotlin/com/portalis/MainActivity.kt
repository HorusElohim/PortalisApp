package com.portalis

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.MediaMetadataRetriever
import android.net.Uri
import android.os.Build
import android.provider.OpenableColumns
import android.util.Size
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.android.FlutterActivity
import io.flutter.plugin.common.MethodChannel
import java.io.ByteArrayOutputStream
import java.util.concurrent.Executors

class MainActivity: FlutterActivity() {
    private val heicChannel = "app.portalis/heic-preview"
    private val filesChannel = "app.portalis/no-copy-source-picker"
    private val filesRequestCode = 4101
    private var pendingFilesResult: MethodChannel.Result? = null
    private val previewExecutor = Executors.newFixedThreadPool(2)

    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        PortalisNative.install(applicationContext)
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, heicChannel)
            .setMethodCallHandler { call, result ->
                if (call.method != "decode") {
                    result.notImplemented()
                    return@setMethodCallHandler
                }
                val path = call.argument<String>("path")
                val maxPixelSize = call.argument<Int>("maxPixelSize") ?: 1024
                if (path == null) {
                    result.error("invalid_path", "A HEIC path is required.", null)
                    return@setMethodCallHandler
                }
                previewExecutor.execute {
                    val bytes = decode(path, maxPixelSize)
                    runOnUiThread {
                        // A grid may be torn down while a native frame is
                        // decoding. Never answer a MethodChannel owned by a
                        // destroyed Flutter engine; the originating widget
                        // is gone and its future no longer has a consumer.
                        if (isFinishing || isDestroyed) return@runOnUiThread
                        if (bytes == null) {
                            result.error(
                                "decode_failed",
                                "The platform could not decode this HEIC image.",
                                null
                            )
                        } else {
                            result.success(bytes)
                        }
                    }
                }
            }
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, filesChannel)
            .setMethodCallHandler { call, result ->
                if (call.method != "pickFiles") {
                    result.notImplemented()
                    return@setMethodCallHandler
                }
                if (pendingFilesResult != null) {
                    result.error("picker_busy", "A Files selection is already open.", null)
                    return@setMethodCallHandler
                }
                pendingFilesResult = result
                startActivityForResult(
                    Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                        addCategory(Intent.CATEGORY_OPENABLE)
                        type = "*/*"
                        putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
                        addFlags(
                            Intent.FLAG_GRANT_READ_URI_PERMISSION or
                                Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
                        )
                    },
                    filesRequestCode,
                )
            }
    }

    override fun onDestroy() {
        previewExecutor.shutdownNow()
        super.onDestroy()
    }

    @Deprecated("FlutterActivity still dispatches document results here.")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != filesRequestCode) return
        val result = pendingFilesResult ?: return
        pendingFilesResult = null
        if (resultCode != RESULT_OK || data == null) {
            result.success(emptyList<Map<String, Any>>())
            return
        }
        try {
            val uris = buildList {
                data.data?.let(::add)
                data.clipData?.let { clip ->
                    for (index in 0 until clip.itemCount) add(clip.getItemAt(index).uri)
                }
            }.distinct()
            val readGrant = data.flags and Intent.FLAG_GRANT_READ_URI_PERMISSION
            require(readGrant != 0) { "Android Files did not grant read access." }
            result.success(uris.map(::persistedSource))
        } catch (error: Exception) {
            result.error("selection_failed", error.message, null)
        }
    }

    private fun persistedSource(uri: Uri): Map<String, Any> {
        contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
        val projection = arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        val cursor = contentResolver.query(uri, projection, null, null, null)
            ?: error("Android Files could not describe the selected document.")
        cursor.use {
            require(cursor.moveToFirst()) { "Android Files could not describe the selected document." }
            val nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            val sizeIndex = cursor.getColumnIndex(OpenableColumns.SIZE)
            val name = if (nameIndex >= 0) cursor.getString(nameIndex) else null
            val size = if (sizeIndex >= 0 && !cursor.isNull(sizeIndex)) cursor.getLong(sizeIndex) else -1L
            require(!name.isNullOrBlank()) { "Android Files did not provide a document name." }
            require(size >= 0) { "Android Files did not provide a stable document length." }
            return mapOf("name" to name, "path" to uri.toString(), "lengthBytes" to size)
        }
    }

    private fun decode(path: String, maxPixelSize: Int): ByteArray? {
        val target = maxPixelSize.coerceIn(64, 2048)
        val bitmap = if (path.startsWith("content://")) {
            decodeContent(Uri.parse(path), target)
        } else {
            decodeFile(path, target)
        } ?: return null
        return encodeJpeg(bitmap)
    }

    private fun decodeFile(path: String, target: Int): Bitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(path, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) {
            return decodeVideo(target) { it.setDataSource(path) }
        }

        val sample = calculateSample(bounds.outWidth, bounds.outHeight, target)
        val options = BitmapFactory.Options().apply { inSampleSize = sample }
        return BitmapFactory.decodeFile(path, options)
            ?: decodeVideo(target) { it.setDataSource(path) }
    }

    private fun decodeContent(uri: Uri, target: Int): Bitmap? {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            runCatching {
                contentResolver.loadThumbnail(uri, Size(target, target), null)
            }.getOrNull()?.let { return it }
        }

        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        contentResolver.openInputStream(uri)?.use {
            BitmapFactory.decodeStream(it, null, bounds)
        }
        if (bounds.outWidth > 0 && bounds.outHeight > 0) {
            val options = BitmapFactory.Options().apply {
                inSampleSize = calculateSample(bounds.outWidth, bounds.outHeight, target)
            }
            contentResolver.openInputStream(uri)?.use {
                BitmapFactory.decodeStream(it, null, options)
            }?.let { return it }
        }

        return decodeVideo(target) { it.setDataSource(this, uri) }
    }

    private fun decodeVideo(
        target: Int,
        setSource: (MediaMetadataRetriever) -> Unit,
    ): Bitmap? = runCatching {
            val retriever = MediaMetadataRetriever()
            try {
                setSource(retriever)
                val frame = retriever.getFrameAtTime(100_000) ?: return@runCatching null
                val scale = minOf(1.0, target.toDouble() / maxOf(frame.width, frame.height))
                if (scale >= 1.0) {
                    frame
                } else {
                    val scaled = Bitmap.createScaledBitmap(
                        frame,
                        (frame.width * scale).toInt().coerceAtLeast(1),
                        (frame.height * scale).toInt().coerceAtLeast(1),
                        true,
                    )
                    frame.recycle()
                    scaled
                }
            } finally {
                retriever.release()
            }
        }.getOrNull()

    private fun encodeJpeg(bitmap: Bitmap): ByteArray? {
        return ByteArrayOutputStream().use { output ->
            val compressed = bitmap.compress(Bitmap.CompressFormat.JPEG, 90, output)
            bitmap.recycle()
            if (compressed) output.toByteArray() else null
        }
    }

    private fun calculateSample(width: Int, height: Int, target: Int): Int {
        var sample = 1
        while (width / (sample * 2) >= target && height / (sample * 2) >= target) {
            sample *= 2
        }
        return sample
    }
}

/**
 * Gives the Rust torrent engine an application-scoped Android context once.
 * It does not read media, select files, or copy bytes; those operations stay
 * inside the future URI-backed ContentLocation implementation.
 */
private object PortalisNative {
    init {
        System.loadLibrary("backend")
    }

    @JvmStatic
    fun install(context: Context) {
        installContext(context)
    }

    @JvmStatic
    private external fun installContext(context: Context)
}

/**
 * Rust's only path into MediaStore. A completed, verified torrent file hands
 * its absolute path here once; this inserts one MediaStore entry backed by
 * that same file's bytes (a single stream copy into the public collection,
 * exactly as any native gallery-saving app performs — Portalis' original
 * received copy is left untouched and removed by Rust only after this
 * succeeds), and returns the resulting `content://` URI so Rust can record it
 * as the entry's durable native location.
 *
 * Never called for anything Portalis does not already own a complete,
 * verified local copy of — this is an export into the system gallery, not a
 * substitute reader for torrent piece I/O.
 */
private object PortalisGallery {
    @JvmStatic
    fun exportToMediaStore(context: Context, path: String, displayName: String, video: Boolean): String? {
        val resolver = context.contentResolver
        val collection: Uri
        val values = android.content.ContentValues().apply {
            put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, displayName)
            val mimeType = guessMimeType(displayName, video)
            if (mimeType != null) {
                put(android.provider.MediaStore.MediaColumns.MIME_TYPE, mimeType)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                put(
                    android.provider.MediaStore.MediaColumns.RELATIVE_PATH,
                    if (video) "Movies/Portalis" else "Pictures/Portalis",
                )
                put(android.provider.MediaStore.MediaColumns.IS_PENDING, 1)
            }
        }
        collection = if (video) {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                android.provider.MediaStore.Video.Media.getContentUri(android.provider.MediaStore.VOLUME_EXTERNAL_PRIMARY)
            } else {
                android.provider.MediaStore.Video.Media.EXTERNAL_CONTENT_URI
            }
        } else {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                android.provider.MediaStore.Images.Media.getContentUri(android.provider.MediaStore.VOLUME_EXTERNAL_PRIMARY)
            } else {
                android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI
            }
        }
        val itemUri = resolver.insert(collection, values) ?: return null
        try {
            resolver.openOutputStream(itemUri)?.use { output ->
                java.io.FileInputStream(path).use { input ->
                    input.copyTo(output)
                }
            } ?: run {
                resolver.delete(itemUri, null, null)
                return null
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                values.clear()
                values.put(android.provider.MediaStore.MediaColumns.IS_PENDING, 0)
                resolver.update(itemUri, values, null, null)
            }
        } catch (error: Exception) {
            resolver.delete(itemUri, null, null)
            return null
        }
        return itemUri.toString()
    }

    private fun guessMimeType(displayName: String, video: Boolean): String? {
        val extension = displayName.substringAfterLast('.', "").lowercase()
        if (extension.isEmpty()) return null
        return android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension)
            ?: if (video) "video/*" else "image/*"
    }
}
