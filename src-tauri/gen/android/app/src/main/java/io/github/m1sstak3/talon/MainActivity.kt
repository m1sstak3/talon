package io.github.m1sstak3.talon

import android.os.Bundle
import android.view.View
import android.view.WindowManager
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // a wallet: no screenshots, no screen recording, a blank card in Recents — from the first frame (PLAN: FLAG_SECURE
    // before render). The core's capture check relies on this being always on.
    window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // edge to edge: the system no longer shrinks the page for the keyboard, so the page is lifted by the keyboard's
    // height here and the open sheet rises with it (bars stay the page's own business: CSS safe-area insets)
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { v, insets ->
      v.setPadding(0, 0, 0, insets.getInsets(WindowInsetsCompat.Type.ime()).bottom)
      insets
    }
  }

  // secrets to the clipboard, marked sensitive and wiped (SecretClip.kt); the desktop does it in the core (clip.rs)
  override fun onWebViewCreate(webView: WebView) {
    webView.addJavascriptInterface(SecretClip(this), "__cwClip")
  }
}
