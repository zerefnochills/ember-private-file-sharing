package com.ember.mobile

import android.os.Handler
import android.os.Looper
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.camera.core.ImageProxy
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.ember.mobile.core.inspectLink
import com.google.zxing.BinaryBitmap
import com.google.zxing.LuminanceSource
import com.google.zxing.MultiFormatReader
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.DecodeHintType
import com.google.zxing.BarcodeFormat
import com.google.zxing.PlanarYUVLuminanceSource
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

@Composable
fun QrScanner(onLink: (String) -> Unit, onError: () -> Unit) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val cameraExecutor = remember { Executors.newSingleThreadExecutor() }
    val previewView = remember { PreviewView(context) }
    val reported = remember { AtomicBoolean(false) }
    val mainHandler = remember { Handler(Looper.getMainLooper()) }
    val analysis = remember {
        ImageAnalysis.Builder()
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .build()
    }
    val preview = remember {
        Preview.Builder().build().also { it.setSurfaceProvider(previewView.surfaceProvider) }
    }
    val reader = remember { MultiFormatReader().apply { setHints(mapOf(com.google.zxing.DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE))) } }
    val providerFuture = remember { ProcessCameraProvider.getInstance(context) }

    DisposableEffect(lifecycleOwner, analysis, preview) {
        var provider: ProcessCameraProvider? = null
        var disposed = false
        val listener = Runnable {
            try {
                provider = providerFuture.get()
                if (!disposed) {
                    provider?.bindToLifecycle(
                        lifecycleOwner,
                        CameraSelector.DEFAULT_BACK_CAMERA,
                        preview,
                        analysis,
                    )
                }
            } catch (_: Exception) {
                mainHandler.post(onError)
            }
        }
        providerFuture.addListener(listener, ContextCompat.getMainExecutor(context))
        analysis.setAnalyzer(cameraExecutor) { image ->
            analyzeFrame(image, reader, reported, mainHandler, onLink)
        }
        onDispose {
            disposed = true
            analysis.clearAnalyzer()
            provider?.unbind(preview, analysis)
            cameraExecutor.shutdownNow()
        }
    }

    AndroidView(
        factory = { previewView },
        modifier = Modifier.fillMaxWidth().height(320.dp),
    )
}

private fun analyzeFrame(
    image: ImageProxy,
    reader: MultiFormatReader,
    reported: AtomicBoolean,
    mainHandler: Handler,
    onLink: (String) -> Unit,
) {
    var yPlane: ByteArray? = null
    try {
        val plane = image.planes.first()
        val buffer = plane.buffer
        yPlane = ByteArray(buffer.remaining()).also(buffer::get)
        val link = decodeEmberLinkQrFrame(
            yPlane,
            image.width,
            image.height,
            plane.rowStride,
            plane.pixelStride,
            image.imageInfo.rotationDegrees,
            reader,
        )
        if (reported.compareAndSet(false, true)) mainHandler.post { onLink(link) }
    } catch (_: Exception) {
        reader.reset()
    } finally {
        yPlane?.fill(0)
        image.close()
    }
}

/** Decodes and validates one CameraX Y plane. The input frame is wiped before return. */
internal fun decodeEmberLinkQrFrame(
    yPlane: ByteArray,
    width: Int,
    height: Int,
    rowStride: Int,
    pixelStride: Int,
    rotationDegrees: Int,
    reader: MultiFormatReader = MultiFormatReader().apply {
        setHints(mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)))
    },
): String {
    val compact = ByteArray(width * height)
    var rotated = compact
    var rotatedWidth = width
    var rotatedHeight = height
    try {
        require(width > 0 && height > 0 && rowStride > 0 && pixelStride > 0)
        require(rotationDegrees in setOf(0, 90, 180, 270))
        require((height - 1) * rowStride + (width - 1) * pixelStride < yPlane.size)
        for (row in 0 until height) {
            for (column in 0 until width) {
                compact[row * width + column] = yPlane[row * rowStride + column * pixelStride]
            }
        }
        val clockwiseTurns = rotationDegrees / 90
        if (clockwiseTurns != 0) {
            val nextWidth = if (clockwiseTurns % 2 == 0) width else height
            val nextHeight = if (clockwiseTurns % 2 == 0) height else width
            val next = ByteArray(rotated.size)
            for (row in 0 until height) {
                for (column in 0 until width) {
                    val (nextX, nextY) = when (clockwiseTurns) {
                        1 -> height - 1 - row to column
                        2 -> width - 1 - column to height - 1 - row
                        else -> row to width - 1 - column
                    }
                    next[nextY * nextWidth + nextX] = compact[row * width + column]
                }
            }
            rotated = next
            rotatedWidth = nextWidth
            rotatedHeight = nextHeight
        }
        val luminance: LuminanceSource = PlanarYUVLuminanceSource(
            rotated,
            rotatedWidth,
            rotatedHeight,
            0,
            0,
            rotatedWidth,
            rotatedHeight,
            false,
        )
        val link = reader.decode(BinaryBitmap(HybridBinarizer(luminance))).text
        inspectLink(link)
        return link
    } finally {
        compact.fill(0)
        if (rotated !== compact) rotated.fill(0)
        yPlane.fill(0)
    }
}
