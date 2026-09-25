package com.streammydrone.app

import android.content.Context
import androidx.annotation.StringRes
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

/**
 * Supported platforms in display order. [brandName] is null for a custom server, which is named
 * in the app's language instead. [defaultServerUrl] is each platform's published ingest address;
 * TikTok and custom servers hand out their own address, so they have none.
 */
enum class DestinationKind(val storageValue: String, val brandName: String?, val defaultServerUrl: String?) {
    INSTAGRAM("instagram", "Instagram", "rtmps://live-upload.instagram.com:443/rtmp"),
    TIKTOK("tiktok", "TikTok", null),
    YOUTUBE("youtube", "YouTube", "rtmps://a.rtmps.youtube.com/live2"),
    FACEBOOK("facebook", "Facebook", "rtmps://live-api-s.facebook.com:443/rtmp"),
    TWITCH("twitch", "Twitch", "rtmp://live.twitch.tv/app"),
    KICK("kick", "Kick", "rtmps://fa723fc1b171.global-contribute.live-video.net:443/app"),
    CUSTOM("custom", null, null);

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
    /** Where the next broadcast goes, in the order the user picked them. */
    val selectedProfileIds: List<String> = emptyList(),
) {
    val selectedProfiles: List<DestinationProfile>
        get() = selectedProfileIds.mapNotNull { id -> profiles.firstOrNull { it.id == id } }

    fun isSelected(profileId: String): Boolean = profileId in selectedProfileIds
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
            throw DestinationProfileException(R.string.profile_error_not_found)
        }

        val id = existingId ?: UUID.randomUUID().toString()
        val encryptedSecret = if (streamKey.isNotEmpty()) {
            validateStreamKey(streamKey)
            try {
                cipher.encrypt(id, streamKey)
            } catch (_: Exception) {
                throw DestinationProfileException(R.string.profile_error_key_encrypt)
            }
        } else {
            if (recordIndex < 0) {
                throw DestinationProfileException(R.string.profile_error_key_required)
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
        // A platform the user just added is one they mean to stream to.
        val updated = StoredProfiles(
            profiles = updatedProfiles,
            selectedProfileIds = if (recordIndex < 0) current.selectedProfileIds + id else current.selectedProfileIds,
        )
        writeRecords(updated)
        updated.toPublicState()
    }

    /** Adds [profileId] to the platforms the next broadcast goes to, or takes it out. */
    fun setSelected(profileId: String, selected: Boolean): DestinationProfiles = synchronized(STORE_LOCK) {
        val current = readRecords()
        if (current.profiles.none { it.id == profileId }) {
            throw DestinationProfileException(R.string.profile_error_not_found)
        }
        val others = current.selectedProfileIds - profileId
        val updated = current.copy(selectedProfileIds = if (selected) others + profileId else others)
        writeRecords(updated)
        updated.toPublicState()
    }

    fun delete(profileId: String): DestinationProfiles = synchronized(STORE_LOCK) {
        val current = readRecords()
        if (current.profiles.none { it.id == profileId }) {
            throw DestinationProfileException(R.string.profile_error_not_found)
        }
        val updatedProfiles = current.profiles.filterNot { it.id == profileId }
        val updated = StoredProfiles(
            profiles = updatedProfiles,
            selectedProfileIds = current.selectedProfileIds - profileId,
        )
        writeRecords(updated)
        if (updatedProfiles.isEmpty()) runCatching { cipher.deleteKey() }
        updated.toPublicState()
    }

    fun credentials(profileId: String): DestinationCredentials = synchronized(STORE_LOCK) {
        val profile = readRecords().profiles.firstOrNull { it.id == profileId }
            ?: throw DestinationProfileException(R.string.profile_error_not_found)
        val streamKey = try {
            cipher.decrypt(profile.id, profile.encryptedSecret)
        } catch (_: Exception) {
            throw DestinationProfileException(R.string.profile_error_key_decrypt)
        }
        DestinationCredentials(profile.serverUrl, streamKey)
    }

    private fun readRecords(): StoredProfiles {
        if (!storage.baseFile.exists()) return StoredProfiles()
        val raw = try {
            storage.openRead().bufferedReader(Charsets.UTF_8).use { it.readText() }
        } catch (_: Exception) {
            throw DestinationProfileException(R.string.profile_error_read)
        }
        return try {
            val root = JSONObject(raw)
            if (root.optInt("version") != STORAGE_VERSION) {
                throw DestinationProfileException(R.string.profile_error_version)
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
            // Files written before several platforms could be picked hold one selectedProfileId.
            val selectedIds = root.optJSONArray("selectedProfileIds")
                ?.let { ids -> List(ids.length()) { index -> ids.optString(index) } }
                ?: listOfNotNull(root.optString("selectedProfileId").takeIf(String::isNotBlank))
            StoredProfiles(profiles, selectedIds.distinct().filter { id -> profiles.any { it.id == id } })
        } catch (error: DestinationProfileException) {
            throw error
        } catch (_: Exception) {
            throw DestinationProfileException(R.string.profile_error_corrupt)
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
            .put("selectedProfileIds", JSONArray(value.selectedProfileIds))
            .put("profiles", profilesJson)
            .toString()
            .toByteArray(Charsets.UTF_8)
        val output = try {
            storage.startWrite()
        } catch (_: Exception) {
            throw DestinationProfileException(R.string.profile_error_write)
        }
        try {
            output.write(bytes)
            storage.finishWrite(output)
        } catch (_: Exception) {
            storage.failWrite(output)
            throw DestinationProfileException(R.string.profile_error_write)
        }
    }

    private fun StoredProfiles.toPublicState(): DestinationProfiles = DestinationProfiles(
        profiles = profiles.map { profile ->
            DestinationProfile(profile.id, profile.name, profile.kind, profile.serverUrl)
        },
        selectedProfileIds = selectedProfileIds,
    )

    companion object {
        private const val PROFILE_FILE_NAME = "destination-profiles-v1.json"
        private const val STORAGE_VERSION = 1
        private val STORE_LOCK = Any()
    }
}

/** A problem with the saved platforms, in words for the screen. */
class DestinationProfileException(val text: UiText) : Exception() {
    constructor(@StringRes id: Int) : this(uiText(id))
}

private data class StoredProfiles(
    val profiles: List<StoredProfile> = emptyList(),
    val selectedProfileIds: List<String> = emptyList(),
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

internal fun validateName(value: String): String {
    val normalized = value.trim()
    if (normalized.isEmpty() || normalized.length > 64 || normalized.any(Char::isISOControl)) {
        throw DestinationProfileException(R.string.profile_error_name)
    }
    return normalized
}

internal fun validateServerUrl(value: String): String {
    val normalized = value.trim().trimEnd('/')
    val authorityAndApp = when {
        normalized.startsWith("rtmps://") -> normalized.removePrefix("rtmps://")
        normalized.startsWith("rtmp://") -> normalized.removePrefix("rtmp://")
        else -> throw DestinationProfileException(R.string.error_destination_scheme)
    }
    val separator = authorityAndApp.indexOf('/')
    val authority = authorityAndApp.take(separator.coerceAtLeast(0))
    val app = if (separator >= 0) authorityAndApp.substring(separator + 1) else ""
    if (
        authority.isEmpty() || app.isEmpty() || authority.contains('@') ||
        normalized.contains('?') || normalized.contains('#') || normalized.any(Char::isWhitespace)
    ) {
        throw DestinationProfileException(R.string.error_destination_server)
    }
    return normalized
}

internal fun validateStreamKey(value: String) {
    val invalid = value.length !in 4..512 || value.any { character ->
        character.code > 127 || character.isWhitespace() || character.isISOControl() ||
            character == '/' || character == '#'
    }
    if (invalid) throw DestinationProfileException(R.string.error_stream_key)
}
