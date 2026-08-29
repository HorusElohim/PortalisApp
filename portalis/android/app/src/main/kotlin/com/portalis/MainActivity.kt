package com.portalis

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.provider.OpenableColumns
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.android.FlutterActivity
import io.flutter.plugin.common.MethodChannel
import java.io.ByteArrayOutputStream

class MainActivity: FlutterActivity() {
    private val heicChannel = "app.portalis/heic-preview"
    private val filesChannel = "app.portalis/no-copy-source-picker"
    private val filesRequestCode = 4101
    private var pendingFilesResult: MethodChannel.Result? = null

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
                Thread {
                    val bytes = decode(path, maxPixelSize)
                    runOnUiThread {
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
                }.start()
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
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(path, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null

        val target = maxPixelSize.coerceIn(64, 2048)
        val sample = calculateSample(bounds.outWidth, bounds.outHeight, target)
        val options = BitmapFactory.Options().apply { inSampleSize = sample }
        val bitmap = BitmapFactory.decodeFile(path, options) ?: return null
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
