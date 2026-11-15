package com.ember.mobile

import com.ember.mobile.core.EmberCrypto
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.net.HttpURLConnection
import java.net.URL

class TextDropTransferTest {
    @Test
    fun burnTextSendsClaimsDecryptsAndFinishesOnlyAfterVerification() {
        val server = FakeRelayServer(burn = true)
        val host = "relay.ember.example"
        val link = TextDropTransfer.sendWithRelay(host, "private text ✅", null, true, 3_600, server.client(host))

        val prepared = TextDropTransfer.prepareReceive(link, null) { server.client(it) }
        val plaintext = prepared.use { TextDropTransfer.receive(it) }

        assertEquals("private text ✅", plaintext)
        assertEquals(2, server.parts.size)
        assertEquals(true, server.burn)
        assertEquals(1, server.claimCalls)
        assertEquals(1, server.finishCalls)
    }

    @Test
    fun tamperedTextIsNeverFinishedOrReturned() {
        val server = FakeRelayServer(burn = true)
        val host = "relay.ember.example"
        val link = TextDropTransfer.sendWithRelay(host, "must not escape", null, true, 3_600, server.client(host))
        server.tamperPartIndex = 1
        val prepared = TextDropTransfer.prepareReceive(link, null) { server.client(it) }

        assertThrows(Exception::class.java) { prepared.use { TextDropTransfer.receive(it) } }
        assertEquals(0, server.finishCalls)
    }

    @Test
    fun filePayloadIsRejectedBeforeFinish() {
        val server = FakeRelayServer(burn = true)
        val host = "relay.ember.example"
        val link = EmberCrypto.generate().use { crypto ->
            val now = System.currentTimeMillis() / 1_000
            val ciphertext = crypto.encryptFile(
                "small.bin",
                "application/octet-stream",
                byteArrayOf(1, 2, 3, 4),
                true,
                now.toULong(),
                (now + 600).toULong(),
            )
            try {
                server.seedCiphertext(ciphertext)
                crypto.buildLink(host, "AAAAAAAAAAAAAAAAAAAAAA")
            } finally {
                ciphertext.fill(0)
            }
        }

        val prepared = TextDropTransfer.prepareReceive(link, null) { server.client(it) }

        assertThrows(UnsupportedDropException::class.java) {
            prepared.use { TextDropTransfer.receive(it) }
        }
        assertEquals(1, server.claimCalls)
        assertEquals(0, server.finishCalls)
    }
}

private class FakeRelayServer(val burn: Boolean) {
    private val blobId = "AAAAAAAAAAAAAAAAAAAAAA"
    private val deleteToken = "fake-delete-token"
    private val claimToken = "fake-claim-token"
    val parts = sortedMapOf<Int, ByteArray>()
    var totalSize: Long = 0
    var state = "uploading"
    var claimCalls = 0
    var finishCalls = 0
    var tamperPartIndex: Int? = null

    fun client(host: String) = RelayClient("https://$host") { url -> FakeConnection(url, this) }

    fun seedCiphertext(ciphertext: ByteArray) {
        val encryptedParts = TextDropTransfer.splitCiphertext(ciphertext)
        encryptedParts.forEachIndexed { index, part -> parts[index] = part }
        totalSize = ciphertext.size.toLong()
        state = "available"
    }

    fun handle(request: FakeConnection): FakeResponse {
        val path = request.url.path
        return when {
            request.requestMethod == "GET" && path == "/v1/pow/challenge" ->
                json(200, """{"challenge":"unit-test-challenge","difficulty":0,"expires_at":4102444800}""")

            request.requestMethod == "POST" && path == "/v1/blobs" -> {
                val body = JSONObject(request.requestBytes.toString(Charsets.UTF_8))
                totalSize = body.getLong("size")
                check(body.getInt("parts") == 2)
                check(body.getBoolean("burn") == burn)
                check(request.getRequestProperty("X-Pow-Challenge") == "unit-test-challenge")
                check(request.getRequestProperty("X-Pow-Nonce") == "0")
                json(
                    200,
                    JSONObject()
                        .put("id", blobId)
                        .put("delete_token", deleteToken)
                        .put("expires_at", 4_102_444_800L)
                        .put("max_part_bytes", RelayClient.MAX_PART_BYTES)
                        .toString(),
                )
            }

            request.requestMethod == "PUT" && path.matches(Regex("/v1/blobs/$blobId/parts/\\d+")) -> {
                check(request.getRequestProperty("Authorization") == "Bearer $deleteToken")
                val index = path.substringAfterLast('/').toInt()
                parts[index] = request.requestBytes.toByteArray()
                FakeResponse(204, ByteArray(0))
            }

            request.requestMethod == "POST" && path == "/v1/blobs/$blobId/complete" -> {
                check(request.getRequestProperty("Authorization") == "Bearer $deleteToken")
                state = "available"
                FakeResponse(200, ByteArray(0))
            }

            request.requestMethod == "GET" && path == "/v1/blobs/$blobId" ->
                json(
                    200,
                    JSONObject()
                        .put("size", totalSize)
                        .put("parts", parts.size)
                        .put("burn", burn)
                        .put("expires_at", 4_102_444_800L)
                        .put("state", state)
                        .toString(),
                )

            request.requestMethod == "POST" && path == "/v1/blobs/$blobId/claim" -> {
                claimCalls++
                json(200, JSONObject().put("claim", if (burn) claimToken else JSONObject.NULL).toString())
            }

            request.requestMethod == "GET" && path.matches(Regex("/v1/blobs/$blobId/parts/\\d+")) -> {
                if (burn) check(request.getRequestProperty("X-Claim") == claimToken)
                val index = path.substringAfterLast('/').toInt()
                val data = requireNotNull(parts[index]).copyOf()
                if (tamperPartIndex == index) data[data.lastIndex] = (data.last().toInt() xor 1).toByte()
                FakeResponse(200, data)
            }

            request.requestMethod == "POST" && path == "/v1/blobs/$blobId/finish" -> {
                if (burn) check(request.getRequestProperty("X-Claim") == claimToken)
                finishCalls++
                FakeResponse(200, ByteArray(0))
            }

            request.requestMethod == "DELETE" && path == "/v1/blobs/$blobId" ->
                FakeResponse(200, ByteArray(0))

            else -> error("Unexpected fake relay request: ${request.requestMethod} ${request.url.path}")
        }
    }

    private fun json(status: Int, text: String) = FakeResponse(status, text.toByteArray(Charsets.UTF_8))
}

private data class FakeResponse(val status: Int, val bytes: ByteArray)

private class FakeConnection(url: URL, private val server: FakeRelayServer) : HttpURLConnection(url) {
    val requestBytes = ByteArrayOutputStream()
    private var response: FakeResponse? = null

    override fun connect() = Unit
    override fun disconnect() = Unit
    override fun usingProxy() = false
    override fun getOutputStream(): OutputStream = requestBytes
    override fun getResponseCode(): Int = response().status
    override fun getInputStream(): InputStream = ByteArrayInputStream(response().bytes)

    private fun response(): FakeResponse {
        response?.let { return it }
        return server.handle(this).also { response = it }
    }
}
