package io.github.m1sstak3.talon

import android.app.Activity
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@InvokeArg
class BioArgs {
  var password: String? = null
  var aad: String = ""
}

// Fingerprint unlock (owner 10.10; design reviewed by Codex 10.10). The wallet password is kept encrypted by an
// AndroidKeyStore AES-GCM key that works only right after a strong biometric (no PIN, no weak face). The core calls
// this plugin and gets the password back itself: the page never sees it, it learns only «unlocked» or an error.
// Unlock only: a send still needs the password typed. A new fingerprint enrolled invalidates the key (system).
@TauriPlugin
class BioPlugin(private val activity: Activity) : Plugin(activity) {
  private val alias = "talon.bio.v1"
  private val blob get() = File(activity.noBackupFilesDir, "bio.bin")
  @Volatile private var busy = false // one prompt at a time

  private fun available() = BiometricManager.from(activity).canAuthenticate(BIOMETRIC_STRONG) == BiometricManager.BIOMETRIC_SUCCESS
  private fun store() = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
  private fun forget() { blob.delete(); runCatching { store().deleteEntry(alias) } }

  private fun newKey(): SecretKey {
    fun spec(strongBox: Boolean) = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
      .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).setKeySize(256)
      .setUserAuthenticationRequired(true).setInvalidatedByBiometricEnrollment(true)
      .apply {
        if (Build.VERSION.SDK_INT >= 30) setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
        else @Suppress("DEPRECATION") setUserAuthenticationValidityDurationSeconds(-1)
        if (strongBox && Build.VERSION.SDK_INT >= 28) setIsStrongBoxBacked(true)
      }.build()
    val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
    return try { gen.init(spec(true)); gen.generateKey() } catch (e: Exception) { gen.init(spec(false)); gen.generateKey() } // no StrongBox chip: the TEE
  }

  // the system sheet with the fingerprint; `done` runs with the cipher it authorised
  private fun prompt(invoke: Invoke, cipher: Cipher, title: String, done: (Cipher) -> Unit) {
    val fa = activity as FragmentActivity
    fa.runOnUiThread {
      BiometricPrompt(fa, ContextCompat.getMainExecutor(fa), object : BiometricPrompt.AuthenticationCallback() {
        override fun onAuthenticationSucceeded(r: BiometricPrompt.AuthenticationResult) {
          busy = false
          val c = r.cryptoObject?.cipher ?: return invoke.reject("отпечаток не подтверждён")
          try { done(c) } catch (e: Exception) { invoke.reject("не удалось открыть: ${e.javaClass.simpleName}") }
        }
        override fun onAuthenticationError(code: Int, msg: CharSequence) { busy = false; invoke.reject(msg.toString(), "cancelled") }
      }).authenticate(
        BiometricPrompt.PromptInfo.Builder().setTitle(title).setNegativeButtonText("Пароль").setAllowedAuthenticators(BIOMETRIC_STRONG).setConfirmationRequired(false).build(),
        BiometricPrompt.CryptoObject(cipher))
    }
  }

  @Command
  fun state(invoke: Invoke) {
    val ok = available()
    invoke.resolve(JSObject().put("available", ok).put("enabled", ok && blob.exists()))
  }

  @Command
  fun enable(invoke: Invoke) {
    val a = invoke.parseArgs(BioArgs::class.java)
    val pw = a.password ?: return invoke.reject("нет пароля")
    if (!available()) return invoke.reject("отпечаток недоступен на этом телефоне")
    if (busy) return invoke.reject("уже ждём отпечаток")
    busy = true
    try {
      forget()
      val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, newKey()) } // fresh key, fresh IV
      prompt(invoke, c, "Вход по отпечатку") { ok ->
        ok.updateAAD(a.aad.toByteArray())
        val bytes = pw.toByteArray()
        val ct = ok.doFinal(bytes); bytes.fill(0)
        val iv = ok.iv
        val tmp = File(blob.path + ".tmp")
        tmp.writeBytes(byteArrayOf(iv.size.toByte()) + iv + ct)
        if (!tmp.renameTo(blob)) { tmp.delete(); throw IllegalStateException("write") }
        invoke.resolve()
      }
    } catch (e: Exception) { busy = false; forget(); invoke.reject("не удалось включить: ${e.javaClass.simpleName}") }
  }

  // ponytail: the password comes back as a String (JNI hands it to the core, which zeroizes its copy); a String
  // cannot be wiped, the byte array is
  @Command
  fun unlock(invoke: Invoke) {
    val a = invoke.parseArgs(BioArgs::class.java)
    if (!blob.exists()) return invoke.reject("отпечаток не включён")
    if (busy) return invoke.reject("уже ждём отпечаток")
    busy = true
    try {
      val raw = blob.readBytes()
      val n = raw[0].toInt()
      val key = store().getKey(alias, null) as? SecretKey ?: throw KeyPermanentlyInvalidatedException()
      val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, raw.copyOfRange(1, 1 + n))) }
      prompt(invoke, c, "Вход в Talon") { ok ->
        ok.updateAAD(a.aad.toByteArray())
        val pw = ok.doFinal(raw.copyOfRange(1 + n, raw.size))
        invoke.resolve(JSObject().put("password", String(pw)))
        pw.fill(0)
      }
    } catch (e: KeyPermanentlyInvalidatedException) { busy = false; forget(); invoke.reject("отпечатки на телефоне изменились: войдите паролем и включите снова", "reset") }
    catch (e: Exception) { busy = false; invoke.reject("не удалось открыть: ${e.javaClass.simpleName}") }
  }

  @Command
  fun disable(invoke: Invoke) { forget(); invoke.resolve() }
}
