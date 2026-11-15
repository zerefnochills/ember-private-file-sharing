package com.ember.mobile

import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.MultiFormatReader
import com.google.zxing.RGBLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeWriter
import org.junit.Assert.assertEquals
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
}
