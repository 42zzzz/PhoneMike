package com.phonemike.app.audio

import android.util.Log
import com.theeasiestway.opus.Constants
import com.theeasiestway.opus.Opus

/**
 * Thin wrapper around the theeasiestway/android-opus-codec native Opus encoder.
 *
 * opus.aar must be placed in app/libs/ (bundled in repo under app/libs/opus.aar).
 *
 * Encoding: converts PCM i16 LE bytes → length-prefixed Opus packet bytes
 * for the TCP stream: [u16 LE frame_len][frame bytes].
 *
 * Frame size: 20ms at 48kHz mono = 960 samples. Must match OPUS_FRAME_SAMPLES.
 */
class OpusEncoder(sampleRate: Int, channels: Int, bitrate: Int = 24000) {

    companion object {
        private const val TAG = "OpusEncoder"

        // 20ms frame at 48kHz
        const val OPUS_FRAME_SAMPLES = 960

        // Frame in bytes (mono i16 = 2 bytes/sample)
        const val OPUS_FRAME_BYTES = OPUS_FRAME_SAMPLES * 2 // mono

        private fun tryLoad(): Boolean = try {
            System.loadLibrary("easyopus")
            true
        } catch (e: UnsatisfiedLinkError) {
            Log.w(TAG, "libeasyopus.so not found — Opus disabled: ${e.message}")
            false
        }

        val isAvailable: Boolean = tryLoad()
    }

    private val opus: Opus? = if (isAvailable) {
        try {
            val sr = when (sampleRate) {
                8000  -> Constants.SampleRate._8000()
                12000 -> Constants.SampleRate._12000()
                16000 -> Constants.SampleRate._16000()
                24000 -> Constants.SampleRate._24000()
                else  -> Constants.SampleRate._48000()
            }
            val ch = if (channels == 1) Constants.Channels.mono() else Constants.Channels.stereo()
            Opus().also {
                it.encoderInit(sr, ch, Constants.Application.voip())
                it.encoderSetBitrate(Constants.Bitrate.instance(bitrate))
            }
        } catch (e: Exception) {
            Log.e(TAG, "Opus encoder init failed: ${e.message}")
            null
        }
    } else null

    val enabled: Boolean get() = opus != null

    /**
     * Encode one 20ms PCM frame (OPUS_FRAME_BYTES bytes) → length-prefixed packet.
     * Returns null on error. Input must be exactly OPUS_FRAME_BYTES bytes.
     */
    fun encodeFrame(pcmFrame: ByteArray): ByteArray? {
        val enc = opus ?: return null
        return try {
            val packet: ByteArray = enc.encode(pcmFrame, Constants.FrameSize._960()) ?: return null
            // Length-prefix framing: [u16 LE frame_len][frame bytes]
            val out = ByteArray(2 + packet.size)
            out[0] = (packet.size and 0xFF).toByte()
            out[1] = ((packet.size shr 8) and 0xFF).toByte()
            packet.copyInto(out, 2)
            out
        } catch (e: Exception) {
            Log.w(TAG, "Opus encode error: ${e.message}")
            null
        }
    }

    fun release() {
        try { opus?.encoderRelease() } catch (_: Exception) {}
    }
}