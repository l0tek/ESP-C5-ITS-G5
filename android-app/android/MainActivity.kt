package dev.dioxus.main

import android.app.Activity
import android.Manifest
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattDescriptor
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.ParcelUuid
import android.provider.OpenableColumns
import androidx.activity.result.contract.ActivityResultContracts
import java.io.ByteArrayOutputStream
import java.util.UUID
import java.util.concurrent.ConcurrentLinkedQueue

typealias BuildConfig = org.l0tek.citsmap.BuildConfig

class MainActivity : WryActivity() {
    private val citsServiceUuid = UUID.fromString("c1750001-6c6f-7465-6b2d-636974736735")
    private val citsDataUuid = UUID.fromString("c1750002-6c6f-7465-6b2d-636974736735")
    private val cccdUuid = UUID.fromString("00002902-0000-1000-8000-00805f9b34fb")
    @Volatile private var citsBleStatus = -1
    @Volatile private var citsBleError = ""
    @Volatile private var citsBleGatt: BluetoothGatt? = null
    @Volatile private var citsBleStopping = false
    private val citsBleQueue = ConcurrentLinkedQueue<ByteArray>()
    private val citsHandler = Handler(Looper.getMainLooper())

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

    private val citsBlePermissions = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { grants ->
        if (grants.values.all { it }) beginCitsBleScan()
        else failCitsBle("Bluetooth-Berechtigung wurde nicht erteilt")
    }

    private val citsGattCallback = object : BluetoothGattCallback() {
        override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
            if (newState == BluetoothProfile.STATE_CONNECTED && status == BluetoothGatt.GATT_SUCCESS) {
                if (!gatt.requestMtu(247)) gatt.discoverServices()
            } else if (newState == BluetoothProfile.STATE_DISCONNECTED) {
                gatt.close()
                if (citsBleGatt === gatt) citsBleGatt = null
                if (citsBleStopping) citsBleStatus = 2
                else failCitsBle("Verbindung zum ESP32-C5 wurde getrennt (GATT $status)")
            }
        }

        override fun onMtuChanged(gatt: BluetoothGatt, mtu: Int, status: Int) {
            gatt.discoverServices()
        }

        override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
            val characteristic = gatt.getService(citsServiceUuid)?.getCharacteristic(citsDataUuid)
            if (status != BluetoothGatt.GATT_SUCCESS || characteristic == null) {
                failCitsBle("CITS-Bluetooth-Dienst wurde nicht gefunden")
                gatt.disconnect()
                return
            }
            if (!gatt.setCharacteristicNotification(characteristic, true)) {
                failCitsBle("Bluetooth-Notifications konnten nicht aktiviert werden")
                gatt.disconnect()
                return
            }
            val descriptor = characteristic.getDescriptor(cccdUuid)
            if (descriptor == null) {
                failCitsBle("Bluetooth-Notification-Deskriptor fehlt")
                gatt.disconnect()
                return
            }
            @Suppress("DEPRECATION")
            descriptor.value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
            @Suppress("DEPRECATION")
            if (!gatt.writeDescriptor(descriptor)) {
                failCitsBle("Bluetooth-Notifications konnten nicht abonniert werden")
                gatt.disconnect()
            }
        }

        override fun onDescriptorWrite(gatt: BluetoothGatt, descriptor: BluetoothGattDescriptor, status: Int) {
            if (descriptor.uuid == cccdUuid && status == BluetoothGatt.GATT_SUCCESS) citsBleStatus = 1
            else if (descriptor.uuid == cccdUuid) failCitsBle("Bluetooth-Abonnement fehlgeschlagen (GATT $status)")
        }

        @Deprecated("Deprecated in Android 13")
        override fun onCharacteristicChanged(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic) {
            @Suppress("DEPRECATION")
            characteristic.value?.let { citsBleQueue.add(it.copyOf()) }
        }

        override fun onCharacteristicChanged(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic, value: ByteArray) {
            citsBleQueue.add(value.copyOf())
        }
    }

    private val citsScanCallback = object : ScanCallback() {
        override fun onScanResult(callbackType: Int, result: ScanResult) {
            try {
                val adapter = getSystemService(BluetoothManager::class.java).adapter
                adapter.bluetoothLeScanner?.stopScan(this)
                citsBleGatt = result.device.connectGatt(this@MainActivity, false, citsGattCallback, android.bluetooth.BluetoothDevice.TRANSPORT_LE)
            } catch (error: SecurityException) {
                failCitsBle("Bluetooth-Berechtigung fehlt")
            }
        }

        override fun onScanFailed(errorCode: Int) {
            failCitsBle("Bluetooth-Suche fehlgeschlagen (Code $errorCode)")
        }
    }

    private fun requiredCitsBlePermissions(): Array<String> = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> arrayOf(
            Manifest.permission.BLUETOOTH_SCAN,
            Manifest.permission.BLUETOOTH_CONNECT
        )
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.M -> arrayOf(Manifest.permission.ACCESS_FINE_LOCATION)
        else -> emptyArray()
    }

    private fun beginCitsBleScan() {
        try {
            val adapter = getSystemService(BluetoothManager::class.java).adapter
            if (adapter == null || !adapter.isEnabled) {
                failCitsBle("Bluetooth ist ausgeschaltet oder nicht verfügbar")
                return
            }
            val filter = ScanFilter.Builder().setServiceUuid(ParcelUuid(citsServiceUuid)).build()
            val settings = ScanSettings.Builder().setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY).build()
            adapter.bluetoothLeScanner?.startScan(listOf(filter), settings, citsScanCallback)
                ?: run { failCitsBle("Bluetooth-LE-Scanner ist nicht verfügbar"); return }
            citsHandler.postDelayed({
                if (citsBleStatus == 0 && citsBleGatt == null) {
                    try { adapter.bluetoothLeScanner?.stopScan(citsScanCallback) } catch (_: SecurityException) {}
                    failCitsBle("CITS-G5 wurde nicht gefunden. Firmware und Bluetooth prüfen.")
                }
            }, 15_000)
        } catch (error: SecurityException) {
            failCitsBle("Bluetooth-Berechtigung fehlt")
        }
    }

    private fun failCitsBle(message: String) {
        citsBleError = message
        citsBleStatus = 3
    }

    fun startCitsBle() {
        citsBleQueue.clear()
        citsBleError = ""
        citsBleStopping = false
        citsBleStatus = 0
        runOnUiThread {
            val missing = requiredCitsBlePermissions().filter { checkSelfPermission(it) != PackageManager.PERMISSION_GRANTED }
            if (missing.isEmpty()) beginCitsBleScan()
            else citsBlePermissions.launch(missing.toTypedArray())
        }
    }

    fun stopCitsBle() {
        citsBleStopping = true
        citsBleStatus = 2
        runOnUiThread {
            try {
                getSystemService(BluetoothManager::class.java).adapter?.bluetoothLeScanner?.stopScan(citsScanCallback)
                citsBleGatt?.disconnect()
                citsBleGatt?.close()
                citsBleGatt = null
            } catch (_: SecurityException) {}
        }
    }

    fun getCitsBleStatus(): Int = citsBleStatus
    fun getCitsBleError(): String = citsBleError
    fun takeCitsBleBytes(): ByteArray? = citsBleQueue.poll()

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
