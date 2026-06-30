package com.phonemike.app.tcp

import android.util.Log
import com.phonemike.app.audio.AudioCaptureEngine
import com.phonemike.app.audio.OpusEncoder
import java.io.IOException
import java.io.OutputStream
import java.net.ServerSocket
import java.net.Socket
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/**
 * TCP server that streams PCM (or Opus) audio to a connected PC client.
 *
 * The PC client connects via ADB port forwarding:
 *   adb forward tcp:18501 tcp:18501
 *   phonemike-client --driver
 *
 * Protocol:
 *   - 16-byte PHMC header on connect (format=1 PCM16, or format=2 Opus)
 *   - PCM mode: raw i16 LE PCM chunks
 *   - Opus mode: length-prefixed Opus packets [u16 LE len][bytes]
 *
 * Opus requires opus.aar in app/libs/. Falls back to PCM if unavailable.
 */
class TcpAudioServer {

    companion object {
        private const val TAG = "TcpAudioServer"
        const val PORT = 18501
        private const val QUEUE_CAPACITY = 8
        private const val HEADER_SIZE = 16
        private const val MAGIC = "PHMC"
        private const val FMT_PCM16: Short = 1
        private const val FMT_OPUS: Short = 2
    }

    private val isRunning = AtomicBoolean(false)
    private var serverSocket: ServerSocket? = null
    private var acceptThread: Thread? = null
    private var clientSocket: Socket? = null
    private var writeThread: Thread? = null
    private val queue = ArrayBlockingQueue<ByteArray>(QUEUE_CAPACITY)

    // Opus encoder — created fresh per-server-start
    private var opusEncoder: OpusEncoder? = null

    // Raw PCM queue: capture thread → processing thread (Opus encode offload)
    private val rawPcmQueue = LinkedBlockingQueue<ByteArray>()
    private var processingThread: Thread? = null
    private val sessionReset = java.util.concurrent.atomic.AtomicBoolean(false)

    var onClientConnected: (() -> Unit)? = null
    var onClientDisconnected: (() -> Unit)? = null

    fun start() {
        if (isRunning.getAndSet(true)) return

        queue.clear()
        rawPcmQueue.clear()

        // Try to create Opus encoder
        opusEncoder = if (OpusEncoder.isAvailable) {
            OpusEncoder(AudioCaptureEngine.SAMPLE_RATE, 1, 24000).also {
                if (it.enabled) Log.i(TAG, "Opus encoder ready (24kbps)")
                else Log.w(TAG, "Opus encoder init failed — using PCM")
            }
        } else null

        try {
            serverSocket = ServerSocket(PORT).also { it.reuseAddress = true }
        } catch (e: IOException) {
            Log.e(TAG, "Failed to bind port $PORT: ${e.message}")
            isRunning.set(false)
            return
        }

        Log.i(TAG, "Listening on port $PORT")

        acceptThread = Thread({
            while (isRunning.get()) {
                try {
                    val ss = serverSocket ?: break
                    val client = ss.accept()
                    Log.i(TAG, "Client connected: ${client.remoteSocketAddress}")
                    handleClient(client)
                } catch (e: IOException) {
                    if (isRunning.get()) Log.w(TAG, "Accept error: ${e.message}")
                }
            }
        }, "TcpAudioServer-Accept").also { it.start() }

        startProcessingThread()
    }

    private fun handleClient(socket: Socket) {
        closeClient()
        clientSocket = socket
        queue.clear()
        rawPcmQueue.clear()
        sessionReset.set(true)

        val out: OutputStream
        try {
            out = socket.getOutputStream()
            val usingOpus = opusEncoder?.enabled == true
            out.write(buildHeader(usingOpus))
            out.flush()
            Log.i(TAG, "Sent PHMC header (format=${if (usingOpus) "Opus" else "PCM16"})")
        } catch (e: IOException) {
            Log.e(TAG, "Failed to send header: ${e.message}")
            closeClient()
            return
        }

        onClientConnected?.invoke()

        writeThread = Thread({
            try {
                while (isRunning.get() && !socket.isClosed) {
                    val chunk = queue.poll(10, TimeUnit.MILLISECONDS) ?: continue
                    out.write(chunk)
                }
            } catch (e: IOException) {
                if (isRunning.get()) Log.w(TAG, "TCP write failed: ${e.message}")
            } finally {
                closeClient()
                onClientDisconnected?.invoke()
            }
        }, "TcpAudioServer-Write").also { it.start() }
    }

    /**
     * Called from audio capture thread with raw PCM i16 LE bytes.
     * Lightweight: just copies data to the processing queue.
     * Opus encoding (if enabled) happens on a dedicated processing thread.
     */
    fun enqueue(buffer: ByteArray, bytesRead: Int) {
        if (clientSocket == null || clientSocket?.isClosed == true) return
        val chunk = buffer.copyOf(bytesRead)
        rawPcmQueue.offer(chunk)
    }

    private fun startProcessingThread() {
        processingThread = Thread({
            // Per-session accumulation state (local to this thread, no locking needed)
            val pcmAccum = ByteArray(OpusEncoder.OPUS_FRAME_BYTES * 4)
            var pcmAccumLen = 0

            while (isRunning.get()) {
                val buffer = rawPcmQueue.poll(100, TimeUnit.MILLISECONDS) ?: continue

                // Reset accumulation state on new client session
                if (sessionReset.getAndSet(false)) {
                    pcmAccumLen = 0
                }

                val enc = opusEncoder
                if (enc == null || !enc.enabled) {
                    // Raw PCM path: forward directly to TCP write queue
                    offerToQueue(buffer)
                    continue
                }

                // Opus path: accumulate PCM until we have a full 20ms frame, then encode
                var srcOffset = 0
                val bytesRead = buffer.size
                while (srcOffset < bytesRead) {
                    val remaining = bytesRead - srcOffset
                    val space = OpusEncoder.OPUS_FRAME_BYTES - pcmAccumLen
                    val toCopy = minOf(remaining, space)

                    System.arraycopy(buffer, srcOffset, pcmAccum, pcmAccumLen, toCopy)
                    pcmAccumLen += toCopy
                    srcOffset += toCopy

                    if (pcmAccumLen >= OpusEncoder.OPUS_FRAME_BYTES) {
                        val frame = pcmAccum.copyOf(OpusEncoder.OPUS_FRAME_BYTES)
                        val packet = enc.encodeFrame(frame)
                        if (packet != null) {
                            offerToQueue(packet)
                        }
                        pcmAccumLen = 0
                    }
                }
            }
        }, "TcpAudioServer-Process").also { it.start() }
    }

    private fun offerToQueue(chunk: ByteArray) {
        if (!queue.offer(chunk)) {
            queue.poll()
            queue.offer(chunk)
        }
    }

    fun stop() {
        isRunning.set(false)
        closeClient()
        try { serverSocket?.close() } catch (_: IOException) {}
        serverSocket = null
        acceptThread?.join(1000)
        acceptThread = null
        processingThread?.join(1000)
        processingThread = null
        rawPcmQueue.clear()
        queue.clear()
        opusEncoder?.release()
        opusEncoder = null
        Log.i(TAG, "Stopped")
    }

    private fun closeClient() {
        writeThread?.let { t ->
            try { clientSocket?.close() } catch (_: IOException) {}
            t.join(1000)
        }
        writeThread = null
        clientSocket = null
    }

    val hasClient: Boolean get() = clientSocket?.isClosed == false

    private fun buildHeader(opus: Boolean): ByteArray {
        val buf = ByteBuffer.allocate(HEADER_SIZE).order(ByteOrder.LITTLE_ENDIAN)
        MAGIC.forEach { buf.put(it.code.toByte()) }
        buf.putInt(AudioCaptureEngine.SAMPLE_RATE)
        buf.putShort(1) // channels (mono)
        buf.putShort(if (opus) FMT_OPUS else FMT_PCM16)
        buf.putInt(if (opus) OpusEncoder.OPUS_FRAME_SAMPLES else 2)
        return buf.array()
    }
}
