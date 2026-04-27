package com.phonemike.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import com.phonemike.app.ui.MainScreen
import com.phonemike.app.ui.MainViewModel
import com.phonemike.app.ui.theme.PhoneMikeTheme

class MainActivity : ComponentActivity() {

    private val viewModel: MainViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            PhoneMikeTheme {
                MainScreen(viewModel)
            }
        }
    }
}