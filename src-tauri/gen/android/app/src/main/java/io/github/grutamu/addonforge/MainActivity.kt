package io.github.grutamu.addonforge

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.inputmethod.InputMethodManager
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import kotlin.math.abs

// The Thor is driven by its gamepad. Every pad button and the system Back
// are consumed here and handed to the UI as `afPad("<NAME>")`, so the UI owns
// navigation and Android's fallbacks (B → Back, A → D-pad centre) never fire.
class MainActivity : TauriActivity() {
  private var webView: WebView? = null
  private val main = Handler(Looper.getMainLooper())

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    hideSystemBars()
    // Registered after Tauri's own callback, so it runs first.
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() = send("BACK")
    })
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    // Android draws a translucent "focused view" wash over the whole WebView in
    // key-navigation mode; the UI draws its own focus ring.
    webView.defaultFocusHighlightEnabled = false
    webView.addJavascriptInterface(Bridge(), "AddonForgeAndroid")
  }

  override fun onWindowFocusChanged(hasFocus: Boolean) {
    super.onWindowFocusChanged(hasFocus)
    if (hasFocus) {
      hideSystemBars()
      focusWebView()
    }
  }

  // Full screen: status and navigation bars stay hidden until swiped in.
  private fun hideSystemBars() {
    val c = WindowCompat.getInsetsController(window, window.decorView)
    c.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
    c.hide(WindowInsetsCompat.Type.systemBars())
  }

  // Pad input must land in a focused WebView or the page draws no focus ring.
  // Key presses take Android out of touch mode (which focuses it); a D-pad
  // that reports as a hat, or the stick, never does, so ask for focus here.
  private fun focusWebView() {
    webView?.let { if (!it.hasFocus()) it.requestFocus() }
  }

  private fun send(name: String) {
    main.post {
      focusWebView()
      webView?.evaluateJavascript("window.afPad && window.afPad('$name')", null)
    }
  }

  private fun padName(code: Int): String? = when (code) {
    KeyEvent.KEYCODE_BUTTON_A -> "A"
    KeyEvent.KEYCODE_BUTTON_B -> "B"
    KeyEvent.KEYCODE_BUTTON_X -> "X"
    KeyEvent.KEYCODE_BUTTON_Y -> "Y"
    KeyEvent.KEYCODE_BUTTON_L1 -> "L1"
    KeyEvent.KEYCODE_BUTTON_R1 -> "R1"
    KeyEvent.KEYCODE_BUTTON_L2 -> "L2"
    KeyEvent.KEYCODE_BUTTON_R2 -> "R2"
    KeyEvent.KEYCODE_BUTTON_START -> "START"
    KeyEvent.KEYCODE_BUTTON_SELECT -> "SELECT"
    KeyEvent.KEYCODE_BUTTON_THUMBL -> "L3"
    KeyEvent.KEYCODE_BUTTON_THUMBR -> "R3"
    else -> null
  }

  private fun fromPad(event: KeyEvent): Boolean =
    event.isFromSource(InputDevice.SOURCE_GAMEPAD) || event.isFromSource(InputDevice.SOURCE_JOYSTICK) ||
      (event.isFromSource(InputDevice.SOURCE_DPAD) && !event.isFromSource(InputDevice.SOURCE_KEYBOARD))

  override fun dispatchKeyEvent(event: KeyEvent): Boolean {
    val dir = when (event.keyCode) {
      KeyEvent.KEYCODE_DPAD_UP -> "UP"
      KeyEvent.KEYCODE_DPAD_DOWN -> "DOWN"
      KeyEvent.KEYCODE_DPAD_LEFT -> "LEFT"
      KeyEvent.KEYCODE_DPAD_RIGHT -> "RIGHT"
      KeyEvent.KEYCODE_DPAD_CENTER -> "A"
      else -> null
    }
    // Typing keyboards keep their arrow keys; pads get UI navigation.
    val name = padName(event.keyCode) ?: if (dir != null && fromPad(event)) dir else null
    if (name != null) {
      // Held directions auto-repeat; held buttons fire once.
      if (event.action == KeyEvent.ACTION_DOWN && (dir != null || event.repeatCount == 0)) send(name)
      return true
    }
    return super.dispatchKeyEvent(event)
  }

  // D-pads that report as a hat, and the left stick, become UP/DOWN/LEFT/RIGHT
  // with key-like auto-repeat while held.
  private var heldDir: String? = null
  private val repeat = object : Runnable {
    override fun run() {
      heldDir?.let { send(it); main.postDelayed(this, 110) }
    }
  }

  override fun dispatchGenericMotionEvent(event: MotionEvent): Boolean {
    if (event.isFromSource(InputDevice.SOURCE_JOYSTICK) && event.action == MotionEvent.ACTION_MOVE) {
      val hx = event.getAxisValue(MotionEvent.AXIS_HAT_X)
      val hy = event.getAxisValue(MotionEvent.AXIS_HAT_Y)
      val sx = event.getAxisValue(MotionEvent.AXIS_X)
      val sy = event.getAxisValue(MotionEvent.AXIS_Y)
      val (x, y) = if (abs(hx) > 0.5f || abs(hy) > 0.5f) hx to hy else sx to sy
      val dir = when {
        abs(y) >= abs(x) && y < -0.6f -> "UP"
        abs(y) >= abs(x) && y > 0.6f -> "DOWN"
        x < -0.6f -> "LEFT"
        x > 0.6f -> "RIGHT"
        else -> null
      }
      if (dir != heldDir) {
        main.removeCallbacks(repeat)
        heldDir = dir
        if (dir != null) {
          send(dir)
          main.postDelayed(repeat, 380)
        }
      }
      return true
    }
    return super.dispatchGenericMotionEvent(event)
  }

  inner class Bridge {
    // AddonForge edits <game folder>/Interface/AddOns in shared storage,
    // which another app created, so it needs "All files access".
    @JavascriptInterface
    fun hasFileAccess(): Boolean = Environment.isExternalStorageManager()

    @JavascriptInterface
    fun requestFileAccess() = main.post {
      try {
        startActivity(Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:$packageName")))
      } catch (e: Exception) {
        startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
      }
    }

    // Selecting a search box with A: bring up the keyboard, which a
    // script-driven focus doesn't do on its own.
    @JavascriptInterface
    fun showKeyboard() = main.post {
      webView?.let {
        if (!it.hasFocus()) it.requestFocus()
        (getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).showSoftInput(it, InputMethodManager.SHOW_IMPLICIT)
      }
    }

    @JavascriptInterface
    fun exit() = main.post { finish() }
  }
}
