package com.phonemike.app.service

import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Binder
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import androidx.core.app.NotificationCompat
import com.phonemike.app.MainActivity
import com.phonemike.app.PhoneMikeApp
import com.phonemike.app.R
import com.phonemike.app.audio.AudioCaptureEngine
import com.phonemike.app.tcp.TcpAudioServer
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlin.math.sqrt

class AudioService : Service() {

    inner class ServiceBinder : Binder() {
        fun getService(): AudioService = this@AudioService
    }

    sealed class ServiceState {
        object Idle : ServiceState()
        object Capturing : ServiceState()
        data class Error(val message: String) : ServiceState()
    }

    sealed class TcpState {
        object Disconnected : TcpState()
        object Listening : TcpState()
        object Connected : TcpState()
    }

    private val binder = ServiceBinder()
    private var engine: AudioCaptureEngine? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private lateinit var tcpServer: TcpAudioServer

    private val _state = MutableStateFlow<ServiceState>(ServiceState.Idle)
    val state: StateFlow<ServiceState> = _state.asStateFlow()

    private val _rmsLevel = MutableStateFlow(0f)
    val rmsLevel: StateFlow<Float> = _rmsLevel.asStateFlow()

    private val _tcpState = MutableStateFlow<TcpState>(TcpState.Disconnected)
    val tcpState: StateFlow<TcpState> = _tcpState.asStateFlow()

    override fun onBind(intent: Intent?): IBinder = binder

    override fun onCreate() {
        super.onCreate()
        tcpServer = TcpAudioServer()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_START -> startCapture()
            ACTION_STOP -> stopCapture()
        }
        return START_NOT_STICKY
    }

    private fun startCapture() {
        if (_state.value is ServiceState.Capturing) return

        try {
            val notification = buildNotification()
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                startForeground(
                    PhoneMikeApp.NOTIFICATION_ID,
                    notification,
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
                )
            } else {
                startForeground(PhoneMikeApp.NOTIFICATION_ID, notification)
            }

            acquireWakeLock()

            tcpServer.onClientConnected = {
                Log.i("AudioService", "TCP client connected")
                _tcpState.value = TcpState.Connected
            }
            tcpServer.onClientDisconnected = {
                Log.i("AudioService", "TCP client disconnected")
                _tcpState.value = TcpState.Listening
            }
            tcpServer.start()
            _tcpState.value = TcpState.Listening

            val capture = AudioCaptureEngine()
            capture.onAudioData = { buffer, bytesRead ->
                computeRms(buffer, bytesRead)
                tcpServer.enqueue(buffer, bytesRead)
            }
            capture.start()
            engine = capture

            _state.value = ServiceState.Capturing
        } catch (e: Exception) {
            _state.value = ServiceState.Error(e.message ?: "Unknown error")
            stopSelf()
        }
    }

    private fun stopCapture() {
        engine?.stop()
        engine = null
        tcpServer.stop()
        _tcpState.value = TcpState.Disconnected
        releaseWakeLock()
        _state.value = ServiceState.Idle
        _rmsLevel.value = 0f
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        engine?.stop()
        engine = null
        tcpServer.stop()
        releaseWakeLock()
        super.onDestroy()
    }

    private fun computeRms(buffer: ByteArray, bytesRead: Int) {
        var sum = 0L
        val sampleCount = bytesRead / 2
        for (i in 0 until bytesRead step 2) {
            val sample = (buffer[i].toInt() and 0xFF) or (buffer[i + 1].toInt() shl 8)
            val signed = if (sample > 32767) sample - 65536 else sample
            sum += signed.toLong() * signed.toLong()
        }
        if (sampleCount > 0) {
            _rmsLevel.value = sqrt(sum.toDouble() / sampleCount).toFloat()
        }
    }

    private fun buildNotification() = NotificationCompat.Builder(this, PhoneMikeApp.CHANNEL_ID)
        .setContentTitle(getString(R.string.app_name))
        .setContentText("Capturing audio")
        .setSmallIcon(android.R.drawable.ic_btn_speak_now)
        .setOngoing(true)
        .setContentIntent(
            PendingIntent.getActivity(
                this, 0,
                Intent(this, MainActivity::class.java).apply {
                    flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
                },
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
        )
        .addAction(
            android.R.drawable.ic_media_pause,
            "Stop",
            PendingIntent.getService(
                this, 1,
                Intent(this, AudioService::class.java).apply { action = ACTION_STOP },
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
        )
        .build()

    private fun acquireWakeLock() {
        val pm = getSystemService(POWER_SERVICE) as PowerManager
        wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "PhoneMike::AudioCapture").apply {
            acquire(10 * 60 * 60 * 1000L) // 10 hours max
        }
    }

    private fun releaseWakeLock() {
        wakeLock?.let {
            if (it.isHeld) it.release()
        }
        wakeLock = null
    }

    companion object {
        const val ACTION_START = "com.phonemike.app.ACTION_START"
        const val ACTION_STOP = "com.phonemike.app.ACTION_STOP"
    }
}