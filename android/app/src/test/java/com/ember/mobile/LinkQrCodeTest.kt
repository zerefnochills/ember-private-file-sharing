package com.ember.mobile

import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.MultiFormatReader
import com.google.zxing.RGBLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeWriter
import com.ember.mobile.core.EmberCrypto
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class LinkQrCodeTest {
    @Test
    fun generatedQrPreservesFullLinkIncludingSecretFragment() {
        val link = "https://relay.ember.example/d/AAAAAAAAAAAAAAAAAAAAAA#AQECAwQFBgcICQ"
        val matrix = QRCodeWriter().encode(link, BarcodeFormat.QR_CODE, 512, 512)
        val pixels = IntArray(matrix.width * matrix.height) { index ->
            val x = index % matrix.width
            val y = index / matrix.width
            if (matrix[x, y]) -0x1000000 else -1
        }
        val bitmap = BinaryBitmap(HybridBinarizer(RGBLuminanceSource(matrix.width, matrix.height, pixels)))

        assertEquals(link, MultiFormatReader().decode(bitmap).text)
    }

    @Test
    fun scannerDecodesRotatedYPlaneWithPixelStrideAndWipesTheFrame() {
        val link = EmberCrypto.generate().use { it.buildLink("relay.ember.example", "AAAAAAAAAAAAAAAAAAAAAA") }
        val upright = QRCodeWriter().encode(link, BarcodeFormat.QR_CODE, 512, 512)
        val width = upright.width
        val height = upright.height
        val rowStride = width * 2 + 8
        val pixels = ByteArray(rowStride * height) { 0x55 }
        for (y in 0 until height) {
            for (x in 0 until width) {
                // Camera input is rotated counterclockwise; rotationDegrees tells CameraX to rotate clockwise.
                val rawX = y
                val rawY = width - 1 - x
                pixels[rawY * rowStride + rawX * 2] = if (upright[x, y]) 0 else 0xFF.toByte()
            }
        }

        val decoded = decodeEmberLinkQrFrame(pixels, width, height, rowStride, 2, 90)

        assertEquals(link, decoded)
        assertTrue(pixels.all { it == 0.toByte() })
    }
}
