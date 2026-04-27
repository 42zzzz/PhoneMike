package com.phonemike.app.ui

import android.app.Application
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.IBinder
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.phonemike.app.service.AudioService
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class MainViewModel(application: Application) : AndroidViewModel(application) {

    private var service: AudioService? = null
    private var bound = false

    private val _state = MutableStateFlow<AudioService.ServiceState>(AudioService.ServiceState.Idle)
    val state: StateFlow<AudioService.ServiceState> = _state.asStateFlow()

    private val _rmsLevel = MutableStateFlow(0f)
    val rmsLevel: StateFlow<Float> = _rmsLevel.asStateFlow()

    private val _tcpState = MutableStateFlow<AudioService.TcpState>(AudioService.TcpState.Disconnected)
    val tcpState: StateFlow<AudioService.TcpState> = _tcpState.asStateFlow()

    private val connection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, binder: IBinder?) {
            val svc = (binder as AudioService.ServiceBinder).getService()
            service = svc
            bound = true
            collectServiceFlows(svc)
        }

        override fun onServiceDisconnected(name: ComponentName?) {
            service = null
            bound = false
        }
    }

    init {
        bindService()
    }

    private fun bindService() {
        val ctx = getApplication<Application>()
        val intent = Intent(ctx, AudioService::class.java)
        ctx.bindService(intent, connection, Context.BIND_AUTO_CREATE)
    }

    private fun collectServiceFlows(svc: AudioService) {
        viewModelScope.launch {
            svc.state.collect { _state.value = it }
        }
        viewModelScope.launch {
            svc.rmsLevel.collect { _rmsLevel.value = it }
        }
        viewModelScope.launch {
            svc.tcpState.collect { _tcpState.value = it }
        }
    }

    fun startCapture() {
        val ctx = getApplication<Application>()
        val intent = Intent(ctx, AudioService::class.java).apply {
            action = AudioService.ACTION_START
        }
        ctx.startForegroundService(intent)
    }

    fun stopCapture() {
        val ctx = getApplication<Application>()
        val intent = Intent(ctx, AudioService::class.java).apply {
            action = AudioService.ACTION_STOP
        }
        ctx.startService(intent)
    }

    override fun onCleared() {
        if (bound) {
            getApplication<Application>().unbindService(connection)
            bound = false
        }
        super.onCleared()
    }
}