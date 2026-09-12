package app.floe.floe_client

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.nio.ByteBuffer
import java.nio.charset.StandardCharsets
import java.nio.file.FileAlreadyExistsException
import java.nio.file.Files
import java.nio.file.StandardOpenOption
import java.security.KeyStore
import java.util.Locale
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

class AndroidVaultKeyStore(context: Context) {
    private val root = File(context.noBackupFilesDir, "agent-vault")
    private val keyLock = Any()

    fun load(personId: String, vaultId: String): ByteArray? = synchronized(keyLock) {
        val slot = slot(personId, vaultId) ?: return@synchronized null
        if (!slot.isFile || slot.length() != (NONCE_BYTES + KEY_BYTES + TAG_BYTES).toLong()) {
            return@synchronized null
        }
        return@synchronized try {
            val payload = slot.readBytes()
            val nonce = payload.copyOfRange(0, NONCE_BYTES)
            val ciphertext = payload.copyOfRange(NONCE_BYTES, payload.size)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(
                Cipher.DECRYPT_MODE,
                wrappingKey(createIfMissing = false),
                GCMParameterSpec(TAG_BITS, nonce),
            )
            cipher.updateAAD(namespace(personId, vaultId))
            cipher.doFinal(ciphertext).takeIf { it.size == KEY_BYTES }
        } catch (_: Exception) {
            null
        }
    }

    fun insert(personId: String, vaultId: String, key: ByteArray): Boolean = synchronized(keyLock) {
        if (key.size != KEY_BYTES) return@synchronized false
        val slot = slot(personId, vaultId) ?: return@synchronized false
        if (slot.exists()) return@synchronized false
        val parent = slot.parentFile ?: return@synchronized false
        if ((!parent.exists() && !parent.mkdirs()) || !parent.isDirectory) {
            return@synchronized false
        }
        return@synchronized try {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, wrappingKey(createIfMissing = true))
            cipher.updateAAD(namespace(personId, vaultId))
            val ciphertext = cipher.doFinal(key)
            val nonce = cipher.iv
            if (nonce.size != NONCE_BYTES) return@synchronized false
            val payload = ByteBuffer.allocate(nonce.size + ciphertext.size)
                .put(nonce)
                .put(ciphertext)
                .array()
            Files.newByteChannel(
                slot.toPath(),
                StandardOpenOption.CREATE_NEW,
                StandardOpenOption.WRITE,
            ).use { channel ->
                val buffer = ByteBuffer.wrap(payload)
                while (buffer.hasRemaining()) channel.write(buffer)
                (channel as? java.nio.channels.FileChannel)?.force(true)
            }
            true
        } catch (_: FileAlreadyExistsException) {
            false
        } catch (_: Exception) {
            false
        }
    }

    private fun slot(personId: String, vaultId: String): File? {
        val person = canonicalUuid(personId) ?: return null
        val vault = canonicalUuid(vaultId) ?: return null
        return File(File(root, person), "$vault.bin")
    }

    private fun namespace(personId: String, vaultId: String): ByteArray =
        "$personId/$vaultId".toByteArray(StandardCharsets.UTF_8)

    private fun wrappingKey(createIfMissing: Boolean): SecretKey = synchronized(keyLock) {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        (keyStore.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return@synchronized it }
        if (!createIfMissing) throw IllegalStateException("vault wrapping key is missing")
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(KEY_BITS)
                .setRandomizedEncryptionRequired(true)
                .build(),
        )
        generator.generateKey()
    }

    private fun canonicalUuid(value: String): String? = try {
        val canonical = UUID.fromString(value).toString()
        canonical.takeIf { value == value.lowercase(Locale.ROOT) && value == canonical }
    } catch (_: IllegalArgumentException) {
        null
    }

    companion object {
        private const val ANDROID_KEYSTORE = "AndroidKeyStore"
        private const val KEY_ALIAS = "floe.agent.vault.aes-gcm.v1"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val NONCE_BYTES = 12
        private const val KEY_BYTES = 32
        private const val TAG_BYTES = 16
        private const val TAG_BITS = TAG_BYTES * 8
        private const val KEY_BITS = KEY_BYTES * 8
    }
}
