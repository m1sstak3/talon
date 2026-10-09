package io.github.m1sstak3.talon

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle
import android.webkit.JavascriptInterface

// The 12 words / keys to the clipboard on Android (desktop: clip.rs). Marked sensitive, so Android 13+ hides the
// preview toast and keyboards keep it out of their history, and wiped after CLEAR_S unless something else was copied
// meanwhile. Reached from the bundled page only (window.__cwClip), which already shows the secret it copies.
class SecretClip(private val ctx: Context) {
  private val main = Handler(Looper.getMainLooper())
  private var copiedAt = 0L

  @JavascriptInterface
  fun put(text: String): Int {
    main.post {
      val cm = ctx.getSystemService(ClipboardManager::class.java) ?: return@post
      val clip = ClipData.newPlainText("", text)
      clip.description.extras = PersistableBundle().apply {
        putBoolean(if (Build.VERSION.SDK_INT >= 33) ClipDescription.EXTRA_IS_SENSITIVE else "android.content.extra.IS_SENSITIVE", true)
      }
      cm.setPrimaryClip(clip)
      val mine = System.nanoTime().also { copiedAt = it }
      var changed = false
      val watch = ClipboardManager.OnPrimaryClipChangedListener { changed = true }
      main.post { cm.addPrimaryClipChangedListener(watch) } // after our own change has been delivered
      main.postDelayed({
        cm.removePrimaryClipChangedListener(watch)
        if (!changed && copiedAt == mine) {
          if (Build.VERSION.SDK_INT >= 28) cm.clearPrimaryClip() else cm.setPrimaryClip(ClipData.newPlainText("", ""))
        }
      }, CLEAR_S * 1000L)
    }
    return CLEAR_S
  }

  companion object { const val CLEAR_S = 30 }
}
