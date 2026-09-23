package com.djilivebridge.android

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.KeyStore
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

enum class DestinationKind(val storageValue: String, val label: String) {
    TIKTOK("tiktok", "TikTok"),
    YOUTUBE("youtube", "YouTube"),
    CUSTOM("custom", "Özel RTMP");

    companion object {
        fun fromStorage(value: String): DestinationKind =
            entries.firstOrNull { it.storageValue == value } ?: CUSTOM
    }
}

data class DestinationProfile(
    val id: String,
    val name: String,
    val kind: DestinationKind,
    val serverUrl: String,
)

data class DestinationProfiles(
    val profiles: List<DestinationProfile> = emptyList(),
    val selectedProfileId: String? = null,
) {
    val selectedProfile: DestinationProfile?
        get() = profiles.firstOrNull { it.id == selectedProfileId }
}

data class DestinationCredentials(
    val serverUrl: String,
    val streamKey: String,
)

class DestinationProfileStore(context: Context) {
    private val storage = AtomicFile(File(context.noBackupFilesDir, PROFILE_FILE_NAME))
    private val cipher = AndroidKeystoreProfileCipher()

    fun load(): DestinationProfiles = synchronized(STORE_LOCK) {
        readRecords().toPublicState()
    }

    fun save(
        existingId: String?,
        name: String,
        kind: DestinationKind,
        serverUrl: String,
        streamKey: String,
    ): DestinationProfiles = synchronized(STORE_LOCK) {
        val normalizedName = validateName(name)
        val normalizedUrl = validateServerUrl(serverUrl)
        val current = readRecords()
        val recordIndex = existingId?.let { id -> current.profiles.indexOfFirst { it.id == id } } ?: -1
        if (existingId != null && recordIndex < 0) {
            throw DestinationProfileException("Düzenlenecek hedef profili bulunamadı")
        }

        val id = existingId ?: UUID.randomUUID().toString()
        val encryptedSecret = if (streamKey.isNotEmpty()) {
            validateStreamKey(streamKey)
            try {
                cipher.encrypt(id, streamKey)
            } catch (_: Exception) {
                throw DestinationProfileException(
                    "Yayın anahtarı Android Keystore ile şifrelenemedi",
                )
            }
        } else {
            if (recordIndex < 0) {
                throw DestinationProfileException("Hedef yayın anahtarı gerekli")
            }
            current.profiles[recordIndex].encryptedSecret
        }
        val updatedRecord = StoredProfile(
            id = id,
            name = normalizedName,
            kind = kind,
            serverUrl = normalizedUrl,
            encryptedSecret = encryptedSecret,
        )
        val updatedProfiles = current.profiles.toMutableList().apply {
            if (recordIndex >= 0) set(recordIndex, updatedRecord) else add(updatedRecord)
        }
        val updated = StoredProfiles(
            profiles = updatedProfiles,
            selectedProfileId = current.selectedProfileId ?: id,
        )
        writeRecords(updated)
        updated.toPublicState()
    }

    fun select(profileId: String): DestinationProfiles = synchronized(STORE_LOCK) {
        val current = readRecords()
        if (current.profiles.none { it.id == profileId }) {
            throw DestinationProfileException("Seçilecek hedef profili bulunamadı")
        }
        val updated = current.copy(selectedProfileId = profileId)
        writeRecords(updated)
        updated.toPublicState()
    }

    fun delete(profileId: String): DestinationProfiles = synchronized(STORE_LOCK) {
        val current = readRecords()
        if (current.profiles.none { it.id == profileId }) {
            throw DestinationProfileException("Silinecek hedef profili bulunamadı")
        }
        val updatedProfiles = current.profiles.filterNot { it.id == profileId }
        val updated = StoredProfiles(
            profiles = updatedProfiles,
            selectedProfileId = if (current.selectedProfileId == profileId) {
                updatedProfiles.firstOrNull()?.id
            } else {
                current.selectedProfileId
            },
        )
        writeRecords(updated)
        if (updatedProfiles.isEmpty()) runCatching { cipher.deleteKey() }
        updated.toPublicState()
    }

    fun credentials(profileId: String): DestinationCredentials = synchronized(STORE_LOCK) {
        val profile = readRecords().profiles.firstOrNull { it.id == profileId }
            ?: throw DestinationProfileException("Aktif hedef profili bulunamadı")
        val streamKey = try {
            cipher.decrypt(profile.id, profile.encryptedSecret)
        } catch (_: Exception) {
            throw DestinationProfileException(
                "Hedef yayın anahtarı Android Keystore ile açılamadı. Profilleri silip yeniden oluşturun.",
            )
        }
        DestinationCredentials(profile.serverUrl, streamKey)
    }

    private fun readRecords(): StoredProfiles {
        if (!storage.baseFile.exists()) return StoredProfiles()
        val raw = try {
            storage.openRead().bufferedReader(Charsets.UTF_8).use { it.readText() }
        } catch (_: Exception) {
            throw DestinationProfileException("Hedef profilleri okunamadı")
        }
        return try {
            val root = JSONObject(raw)
            if (root.optInt("version") != STORAGE_VERSION) {
                throw DestinationProfileException("Hedef profili depolama sürümü desteklenmiyor")
            }
            val values = root.getJSONArray("profiles")
            val profiles = buildList {
                for (index in 0 until values.length()) {
                    val item = values.getJSONObject(index)
                    add(
                        StoredProfile(
                            id = item.getString("id"),
                            name = item.getString("name"),
                            kind = DestinationKind.fromStorage(item.optString("kind")),
                            serverUrl = item.getString("serverUrl"),
                            encryptedSecret = EncryptedSecret(
                                iv = Base64.decode(item.getString("iv"), Base64.NO_WRAP),
                                ciphertext = Base64.decode(
                                    item.getString("ciphertext"),
                                    Base64.NO_WRAP,
                                ),
                            ),
                        ),
                    )
                }
            }
            val selectedId = root.optString("selectedProfileId").takeIf(String::isNotBlank)
                ?.takeIf { id -> profiles.any { it.id == id } }
            StoredProfiles(profiles, selectedId ?: profiles.firstOrNull()?.id)
        } catch (error: DestinationProfileException) {
            throw error
        } catch (_: Exception) {
            throw DestinationProfileException("Hedef profili dosyası geçersiz veya bozuk")
        }
    }

    private fun writeRecords(value: StoredProfiles) {
        val profilesJson = JSONArray()
        value.profiles.forEach { profile ->
            profilesJson.put(
                JSONObject()
                    .put("id", profile.id)
                    .put("name", profile.name)
                    .put("kind", profile.kind.storageValue)
                    .put("serverUrl", profile.serverUrl)
                    .put("iv", Base64.encodeToString(profile.encryptedSecret.iv, Base64.NO_WRAP))
                    .put(
                        "ciphertext",
                        Base64.encodeToString(profile.encryptedSecret.ciphertext, Base64.NO_WRAP),
                    ),
            )
        }
        val bytes = JSONObject()
            .put("version", STORAGE_VERSION)
            .put("selectedProfileId", value.selectedProfileId ?: JSONObject.NULL)
            .put("profiles", profilesJson)
            .toString()
            .toByteArray(Charsets.UTF_8)
        val output = try {
            storage.startWrite()
        } catch (_: Exception) {
            throw DestinationProfileException("Hedef profili depolaması açılamadı")
        }
        try {
            output.write(bytes)
            storage.finishWrite(output)
        } catch (_: Exception) {
            storage.failWrite(output)
            throw DestinationProfileException("Hedef profilleri güvenli biçimde kaydedilemedi")
        }
    }

    private fun StoredProfiles.toPublicState(): DestinationProfiles = DestinationProfiles(
        profiles = profiles.map { profile ->
            DestinationProfile(profile.id, profile.name, profile.kind, profile.serverUrl)
        },
        selectedProfileId = selectedProfileId,
    )

    companion object {
        private const val PROFILE_FILE_NAME = "destination-profiles-v1.json"
        private const val STORAGE_VERSION = 1
        private val STORE_LOCK = Any()
    }
}

class DestinationProfileException(message: String) : Exception(message)

private data class StoredProfiles(
    val profiles: List<StoredProfile> = emptyList(),
    val selectedProfileId: String? = null,
)

private data class StoredProfile(
    val id: String,
    val name: String,
    val kind: DestinationKind,
    val serverUrl: String,
    val encryptedSecret: EncryptedSecret,
)

private data class EncryptedSecret(
    val iv: ByteArray,
    val ciphertext: ByteArray,
)

private class AndroidKeystoreProfileCipher {
    fun encrypt(profileId: String, plaintext: String): EncryptedSecret {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, getOrCreateKey())
        cipher.updateAAD(aad(profileId))
        return EncryptedSecret(cipher.iv, cipher.doFinal(plaintext.toByteArray(Charsets.UTF_8)))
    }

    fun decrypt(profileId: String, value: EncryptedSecret): String {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, getOrCreateKey(), GCMParameterSpec(GCM_TAG_BITS, value.iv))
        cipher.updateAAD(aad(profileId))
        return cipher.doFinal(value.ciphertext).toString(Charsets.UTF_8)
    }

    fun deleteKey() {
        KeyStore.getInstance(ANDROID_KEYSTORE).apply {
            load(null)
            if (containsAlias(KEY_ALIAS)) deleteEntry(KEY_ALIAS)
        }
    }

    private fun getOrCreateKey(): SecretKey {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        (keyStore.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setRandomizedEncryptionRequired(true)
                .build(),
        )
        return generator.generateKey()
    }

    private fun aad(profileId: String): ByteArray =
        "dji-live-bridge-destination:$profileId".toByteArray(Charsets.UTF_8)

    companion object {
        private const val ANDROID_KEYSTORE = "AndroidKeyStore"
        private const val KEY_ALIAS = "dji-live-bridge-destination-key-v1"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val GCM_TAG_BITS = 128
    }
}

private fun validateName(value: String): String {
    val normalized = value.trim()
    if (normalized.isEmpty() || normalized.length > 64 || normalized.any(Char::isISOControl)) {
        throw DestinationProfileException("Profil adı 1-64 karakter olmalı")
    }
    return normalized
}

private fun validateServerUrl(value: String): String {
    val normalized = value.trim().trimEnd('/')
    val authorityAndApp = when {
        normalized.startsWith("rtmps://") -> normalized.removePrefix("rtmps://")
        normalized.startsWith("rtmp://") -> normalized.removePrefix("rtmp://")
        else -> throw DestinationProfileException("Sunucu adresi rtmp:// veya rtmps:// ile başlamalı")
    }
    val separator = authorityAndApp.indexOf('/')
    val authority = authorityAndApp.take(separator.coerceAtLeast(0))
    val app = if (separator >= 0) authorityAndApp.substring(separator + 1) else ""
    if (
        authority.isEmpty() || app.isEmpty() || authority.contains('@') ||
        normalized.contains('?') || normalized.contains('#') || normalized.any(Char::isWhitespace)
    ) {
        throw DestinationProfileException("Hedef RTMP sunucu adresi geçersiz")
    }
    return normalized
}

private fun validateStreamKey(value: String) {
    val invalid = value.length !in 4..512 || value.any { character ->
        character.code > 127 || character.isWhitespace() || character.isISOControl() ||
            character == '/' || character == '#'
    }
    if (invalid) throw DestinationProfileException("Hedef yayın anahtarı geçersiz")
}
