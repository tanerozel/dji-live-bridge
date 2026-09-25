package com.streammydrone.app

import android.content.Context
import java.io.File
import java.nio.charset.StandardCharsets
import java.security.KeyStore
import java.util.Base64
import javax.net.ssl.TrustManagerFactory
import javax.net.ssl.X509TrustManager

private const val CA_BUNDLE_FILE_NAME = "rtmps-system-cas.pem"

internal fun createAndroidSystemCaBundle(context: Context): File {
    val factory = TrustManagerFactory.getInstance(TrustManagerFactory.getDefaultAlgorithm())
    factory.init(null as KeyStore?)
    val trustManager = factory.trustManagers
        .filterIsInstance<X509TrustManager>()
        .firstOrNull()
        ?: error("Android system trust store not found")
    val certificates = trustManager.acceptedIssuers
        .filter { certificate -> certificate.basicConstraints >= 0 }
        .distinctBy { certificate -> certificate.encoded.contentHashCode() }
    check(certificates.isNotEmpty()) { "Android system trust store is empty" }

    val output = File(context.noBackupFilesDir, CA_BUNDLE_FILE_NAME)
    val encoder = Base64.getMimeEncoder(64, byteArrayOf('\n'.code.toByte()))
    output.outputStream().bufferedWriter(StandardCharsets.US_ASCII).use { writer ->
        certificates.forEach { certificate ->
            writer.appendLine("-----BEGIN CERTIFICATE-----")
            writer.appendLine(encoder.encodeToString(certificate.encoded))
            writer.appendLine("-----END CERTIFICATE-----")
        }
    }
    return output
}
