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
    try {
        val plane = image.planes.first()
        val buffer = plane.buffer
        val width = image.width
        val height = image.height
        val yPlane = ByteArray(width * height)
        val start = buffer.position()
        for (row in 0 until height) {
            for (column in 0 until width) {
                yPlane[row * width + column] = buffer.get(start + row * plane.rowStride + column * plane.pixelStride)
            }
        }
        var luminance: LuminanceSource = PlanarYUVLuminanceSource(
            yPlane,
            width,
            height,
            0,
            0,
            width,
            height,
            false,
        )
        val counterClockwiseTurns = (4 - (image.imageInfo.rotationDegrees / 90) % 4) % 4
        repeat(counterClockwiseTurns) {
            luminance = luminance.rotateCounterClockwise()
        }
        val text = reader.decode(BinaryBitmap(HybridBinarizer(luminance))).text
        inspectLink(text)
        if (reported.compareAndSet(false, true)) mainHandler.post { onLink(text) }
    } catch (_: Exception) {
        reader.reset()
    } finally {
        image.close()
    }
}
