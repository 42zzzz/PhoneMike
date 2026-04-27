package com.phonemike.app.ui

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.phonemike.app.service.AudioService
import com.phonemike.app.ui.theme.CapturingGreen
import com.phonemike.app.ui.theme.ErrorRed
import com.phonemike.app.ui.theme.ListeningAmber
import com.phonemike.app.ui.theme.RmsBarColor

@Composable
fun MainScreen(viewModel: MainViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val rmsLevel by viewModel.rmsLevel.collectAsStateWithLifecycle()
    val tcpState by viewModel.tcpState.collectAsStateWithLifecycle()
    val context = LocalContext.current

    var hasAudioPermission by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO)
                    == PackageManager.PERMISSION_GRANTED
        )
    }

    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions()
    ) { results ->
        hasAudioPermission = results[Manifest.permission.RECORD_AUDIO] == true
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center
    ) {
        // Status
        Text(
            text = when (state) {
                is AudioService.ServiceState.Idle -> "Idle"
                is AudioService.ServiceState.Capturing -> "Capturing"
                is AudioService.ServiceState.Error -> "Error: ${(state as AudioService.ServiceState.Error).message}"
            },
            color = when (state) {
                is AudioService.ServiceState.Idle -> MaterialTheme.colorScheme.onBackground
                is AudioService.ServiceState.Capturing -> CapturingGreen
                is AudioService.ServiceState.Error -> ErrorRed
            },
            fontSize = 24.sp
        )

        Spacer(modifier = Modifier.height(8.dp))

        // TCP connection status
        Text(
            text = when (tcpState) {
                is AudioService.TcpState.Disconnected -> "Not connected"
                is AudioService.TcpState.Listening -> "Waiting for connection..."
                is AudioService.TcpState.Connected -> "Connected"
            },
            color = when (tcpState) {
                is AudioService.TcpState.Disconnected -> MaterialTheme.colorScheme.onBackground.copy(alpha = 0.5f)
                is AudioService.TcpState.Listening -> ListeningAmber
                is AudioService.TcpState.Connected -> CapturingGreen
            },
            fontSize = 14.sp
        )

        Spacer(modifier = Modifier.height(16.dp))

        // RMS level bar
        Text(
            text = "Level: ${rmsLevel.toInt()}",
            color = MaterialTheme.colorScheme.onBackground,
            fontSize = 14.sp
        )

        Spacer(modifier = Modifier.height(8.dp))

        val normalizedLevel = (rmsLevel / 16384f).coerceIn(0f, 1f)
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(24.dp)
                .clip(RoundedCornerShape(12.dp))
                .background(MaterialTheme.colorScheme.surface)
        ) {
            Box(
                modifier = Modifier
                    .fillMaxWidth(normalizedLevel)
                    .height(24.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(RmsBarColor)
            )
        }

        Spacer(modifier = Modifier.height(32.dp))

        if (!hasAudioPermission) {
            Button(
                onClick = {
                    val permissions = mutableListOf(Manifest.permission.RECORD_AUDIO)
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                        permissions.add(Manifest.permission.POST_NOTIFICATIONS)
                    }
                    permissionLauncher.launch(permissions.toTypedArray())
                },
                colors = ButtonDefaults.buttonColors(
                    containerColor = MaterialTheme.colorScheme.primary
                )
            ) {
                Text("Grant Audio Permission")
            }
        } else {
            val isCapturing = state is AudioService.ServiceState.Capturing
            Button(
                onClick = {
                    if (isCapturing) viewModel.stopCapture() else viewModel.startCapture()
                },
                colors = ButtonDefaults.buttonColors(
                    containerColor = if (isCapturing) ErrorRed else CapturingGreen
                ),
                modifier = Modifier
                    .fillMaxWidth(0.6f)
                    .height(56.dp)
            ) {
                Text(
                    text = if (isCapturing) "Stop" else "Start",
                    fontSize = 18.sp
                )
            }
        }
    }
}