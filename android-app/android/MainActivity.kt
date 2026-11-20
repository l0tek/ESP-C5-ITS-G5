package dev.dioxus.main

import android.app.Activity
import android.content.Intent
import android.provider.OpenableColumns
import androidx.activity.result.contract.ActivityResultContracts
import java.io.ByteArrayOutputStream

typealias BuildConfig = org.l0tek.citsmap.BuildConfig

class MainActivity : WryActivity() {
    @Volatile private var citsStatus = -1
    @Volatile private var citsBytes: ByteArray? = null
    @Volatile private var citsName = "Dateiimport"
    @Volatile private var citsError = ""

    private val citsPicker = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null) {
            citsStatus = 2
        } else {
            Thread {
                try {
                    contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                        if (cursor.moveToFirst()) citsName = cursor.getString(0) ?: "Dateiimport"
                    }
                    val bytes = contentResolver.openInputStream(uri)?.use { input ->
                        val output = ByteArrayOutputStream()
                        val buffer = ByteArray(8192)
                        while (true) {
                            val count = input.read(buffer)
                            if (count < 0) break
                            require(output.size() + count <= 8 * 1024 * 1024) { "Datei größer als 8 MiB" }
                            output.write(buffer, 0, count)
                        }
                        output.toByteArray()
                    } ?: error("Datei konnte nicht geöffnet werden")
                    citsBytes = bytes
                    citsStatus = 1
                } catch (error: Exception) {
                    citsError = error.message ?: "Datei konnte nicht gelesen werden"
                    citsStatus = 3
                }
            }.start()
        }
    }

    fun startCitsImport() {
        citsBytes = null
        citsName = "Dateiimport"
        citsError = ""
        citsStatus = 0
        runOnUiThread {
            try {
                val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = "*/*"
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                citsPicker.launch(intent)
            } catch (error: Exception) {
                citsError = error.message ?: "Dateiauswahl konnte nicht geöffnet werden"
                citsStatus = 3
            }
        }
    }

    fun getCitsImportStatus(): Int = citsStatus
    fun getCitsImportName(): String = citsName
    fun getCitsImportError(): String = citsError
    fun takeCitsImportBytes(): ByteArray? {
        val bytes = citsBytes
        citsBytes = null
        citsStatus = -1
        return bytes
    }
}
