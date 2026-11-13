package com.obdium.app

import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothSocket
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import java.io.InputStream
import java.io.OutputStream
import java.util.UUID

/**
 * Classic Bluetooth serial link used by ELM327 V2.1 dongles.
 * The phone-side name is usually OBDII and the PIN is 1234.
 */
@SuppressLint("MissingPermission")
object ElmBluetooth {
    private val sppUuid: UUID = UUID.fromString("00001101-0000-1000-8000-00805F9B34FB")
    private var appContext: Context? = null
    private var socket: BluetoothSocket? = null
    private var input: InputStream? = null
    private var output: OutputStream? = null

    @JvmStatic
    fun attach(context: Context) {
        appContext = context.applicationContext
    }

    @JvmStatic
    @Synchronized
    fun bondedNames(): Array<String> {
        val adapter = adapter() ?: return arrayOf("BLUETOOTH_UNAVAILABLE")
        if (!canConnect()) return arrayOf("BLUETOOTH_PERMISSION_REQUIRED")
        return try {
            if (!adapter.isEnabled) return arrayOf("BLUETOOTH_DISABLED")
            val names = adapter.bondedDevices.map { device ->
                device.name?.takeIf { it.isNotBlank() } ?: device.address
            }.distinct()
            (listOf("OK") + names).toTypedArray()
        } catch (_: SecurityException) {
            arrayOf("BLUETOOTH_PERMISSION_REQUIRED")
        }
    }

    @JvmStatic
    @Synchronized
    fun connect(target: String): String {
        close()
        val adapter = adapter() ?: return "NO_ADAPTER"
        if (!canConnect()) return "PERMISSION"
        if (!adapter.isEnabled) return "BLUETOOTH_DISABLED"
        val wanted = target.trim()
        val device = adapter.bondedDevices.firstOrNull { candidate ->
            val name = candidate.name ?: ""
            name.equals(wanted, ignoreCase = true) ||
                candidate.address.equals(wanted, ignoreCase = true)
        } ?: return "NOT_FOUND"

        try {
            adapter.cancelDiscovery()
        } catch (_: SecurityException) {
        }

        val attempts = listOf(
            "SPP" to { device.createRfcommSocketToServiceRecord(sppUuid) },
            "SPP compatibility" to { device.createInsecureRfcommSocketToServiceRecord(sppUuid) },
            "RFCOMM channel 1" to {
                val method = device.javaClass.getMethod("createRfcommSocket", Int::class.javaPrimitiveType)
                method.invoke(device, 1) as BluetoothSocket
            },
        )
        val failures = mutableListOf<String>()
        for ((mode, createSocket) in attempts) {
            val error = try {
                open(createSocket())
            } catch (error: Exception) {
                error.message ?: error.javaClass.simpleName
            }
            if (error == null) {
                Log.i("HiRoBluetooth", "$mode connected")
                return "OK"
            }
            failures.add("$mode: $error")
            Log.w("HiRoBluetooth", "$mode failed: $error")
        }
        return failures.joinToString(" | ")
    }

    @JvmStatic
    @Synchronized
    fun write(data: ByteArray) {
        val stream = output ?: throw IllegalStateException("not connected")
        stream.write(data)
        stream.flush()
    }

    @JvmStatic
    @Synchronized
    fun read(max: Int, timeoutMs: Int): ByteArray {
        val stream = input ?: return ByteArray(0)
        val deadline = System.currentTimeMillis() + timeoutMs.coerceAtLeast(20)
        while (System.currentTimeMillis() < deadline) {
            val available = try {
                stream.available()
            } catch (_: Exception) {
                return ByteArray(0)
            }
            if (available > 0) {
                val size = minOf(max.coerceAtLeast(1), available)
                val buffer = ByteArray(size)
                val got = stream.read(buffer)
                if (got <= 0) return ByteArray(0)
                return buffer.copyOf(got)
            }
            try {
                Thread.sleep(8)
            } catch (_: InterruptedException) {
                return ByteArray(0)
            }
        }
        return ByteArray(0)
    }

    @JvmStatic
    @Synchronized
    fun clearInput() {
        val stream = input ?: return
        try {
            while (stream.available() > 0) {
                val pending = stream.available()
                if (pending <= 0) break
                stream.read(ByteArray(pending))
            }
        } catch (_: Exception) {
        }
    }

    @JvmStatic
    @Synchronized
    fun close() {
        try {
            input?.close()
        } catch (_: Exception) {
        }
        try {
            output?.close()
        } catch (_: Exception) {
        }
        try {
            socket?.close()
        } catch (_: Exception) {
        }
        input = null
        output = null
        socket = null
    }

    private fun open(next: BluetoothSocket): String? {
        return try {
            next.connect()
            val nextInput = next.inputStream
            val nextOutput = next.outputStream
            socket = next
            input = nextInput
            output = nextOutput
            null
        } catch (error: Exception) {
            try {
                next.close()
            } catch (_: Exception) {
            }
            error.message ?: "connect failed"
        }
    }

    private fun adapter(): BluetoothAdapter? {
        val context = appContext ?: return null
        val manager = context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager
        return manager?.adapter
    }

    private fun canConnect(): Boolean {
        val context = appContext ?: return false
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return true
        return context.checkSelfPermission(android.Manifest.permission.BLUETOOTH_CONNECT) ==
            PackageManager.PERMISSION_GRANTED
    }
}
