package io.github.m1sstak3.talon

import android.content.ClipboardManager
import android.content.pm.ApplicationInfo
import android.system.Os
import java.io.File
import android.content.Context
import android.os.Build
import android.os.Bundle
import android.view.HapticFeedbackConstants
import android.view.View
import android.view.WindowManager
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  private var web: WebView? = null
  // the system bars and cutout in CSS px; the bottom is 0 while the keyboard is up (the page already ends on it)
  @Volatile var insets = """{"t":0,"b":0,"l":0,"r":0}"""

  override fun onCreate(savedInstanceState: Bundle?) {
    // a wallet: no screenshots, no screen recording, a blank card in Recents — from the first frame (PLAN: FLAG_SECURE
    // before render). The core's capture check relies on this being always on.
    window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
    // a debuggable (development) build keeps its keys in a throwaway vault of its own, as the desktop dev runs do with
    // CW_VAULT; the core then allows the local test network. A release build is never debuggable: never set there.
    if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0) Os.setenv("CW_VAULT", File(filesDir, "dev-vault/vault.json").path, true)
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // edge to edge: the system no longer shrinks the page for the keyboard, so the page is lifted by the keyboard's
    // height here and the open sheet rises with it. The bars go to the page as CSS variables: env(safe-area-inset-*)
    // reads 0 or a few px off in older Android WebViews
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { v, all ->
      val ime = all.getInsets(WindowInsetsCompat.Type.ime()).bottom
      val bars = all.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      v.setPadding(0, 0, 0, ime)
      val d = resources.displayMetrics.density
      insets = """{"t":${bars.top / d},"b":${if (ime > 0) 0f else bars.bottom / d},"l":${bars.left / d},"r":${bars.right / d}}"""
      web?.evaluateJavascript("window.__cwInsets&&__cwInsets($insets)", null)
      all
    }
  }

  // secrets to the clipboard, marked sensitive and wiped (SecretClip.kt); the desktop does it in the core (clip.rs)
  override fun onWebViewCreate(webView: WebView) {
    web = webView
    webView.addJavascriptInterface(SecretClip(this), "__cwClip")
    webView.addJavascriptInterface(AppBridge(this), "__cwApp")
  }
}

// what a phone app does that a page cannot: go to the background on back from home, the bars' size, system haptics
// (performHapticFeedback: no permission, follows the owner's touch-feedback setting)
class AppBridge(private val a: MainActivity) {
  @JavascriptInterface fun home() { a.runOnUiThread { a.moveTaskToBack(true) } }
  @JavascriptInterface fun insets(): String = a.insets
  // «Вставить» in the recipient field: a WebView may not read the clipboard itself; text only, capped
  @JavascriptInterface fun paste(): String {
    val clip = (a.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).primaryClip ?: return ""
    return if (clip.itemCount > 0) clip.getItemAt(0).coerceToText(a).toString().take(200) else ""
  }
  @JavascriptInterface fun haptic(kind: String) {
    a.runOnUiThread {
      a.window.decorView.performHapticFeedback(when (kind) {
        "ok" -> if (Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.VIRTUAL_KEY
        "err" -> if (Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.LONG_PRESS
        else -> HapticFeedbackConstants.CLOCK_TICK
      })
    }
  }
}
