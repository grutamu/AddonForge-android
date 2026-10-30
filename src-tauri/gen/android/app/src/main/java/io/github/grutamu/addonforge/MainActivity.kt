package io.github.grutamu.addonforge

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.provider.DocumentsContract
import android.provider.Settings
import android.view.HapticFeedbackConstants
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.view.inputmethod.InputMethodManager
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.FileProvider
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import org.json.JSONObject
import java.io.File
import kotlin.math.abs

// Pad buttons and Back are consumed and sent to the page as afPad("NAME").
class MainActivity : TauriActivity() {
  private var webView: WebView? = null
  private val main = Handler(Looper.getMainLooper())

  // Picker results go to afResult(id, value); null when cancelled.
  private var pendingId = ""
  private val treePicker = registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
    reply(pendingId, uri?.let { treeToPath(it) })
  }
  private val filePicker = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
    reply(pendingId, uri?.let { copyToCache(it, "import.json") })
  }

  private var launchAction = ""

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    launchAction = intent?.getStringExtra("af_action") ?: ""
    hideSystemBars()
    // Registered after Tauri's own callback, so it runs first.
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() = send("BACK")
    })
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    intent.getStringExtra("af_action")?.let {
      launchAction = it
      main.post { webView?.evaluateJavascript("window.afLaunch && window.afLaunch()", null) }
    }
  }

  private fun reply(id: String, value: String?) {
    val v = if (value == null) "null" else JSONObject.quote(value)
    main.post { webView?.evaluateJavascript("window.afResult && window.afResult(${JSONObject.quote(id)}, $v)", null) }
  }

  // primary:Download/WoW → /storage/emulated/0/Download/WoW, 1A2B-3C4D:WoW → /storage/1A2B-3C4D/WoW
  private fun treeToPath(uri: Uri): String? {
    val id = DocumentsContract.getTreeDocumentId(uri) ?: return null
    val volume = id.substringBefore(':')
    val rel = id.substringAfter(':', "")
    val base = if (volume == "primary") Environment.getExternalStorageDirectory().absolutePath else "/storage/$volume"
    return if (rel.isEmpty()) base else "$base/$rel"
  }

  private fun copyToCache(uri: Uri, name: String): String? = try {
    val out = File(cacheDir, name)
    contentResolver.openInputStream(uri)?.use { input -> out.outputStream().use { input.copyTo(it) } }
    out.absolutePath
  } catch (e: Exception) {
    null
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    // The UI draws its own focus ring.
    webView.defaultFocusHighlightEnabled = false
    webView.addJavascriptInterface(Bridge(), "AddonForgeAndroid")
    // Edge to edge, the window no longer resizes for the keyboard.
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(window.decorView) { v, insets ->
      content.setPadding(0, 0, 0, insets.getInsets(WindowInsetsCompat.Type.ime()).bottom)
      ViewCompat.onApplyWindowInsets(v, insets)
    }
  }

  override fun onWindowFocusChanged(hasFocus: Boolean) {
    super.onWindowFocusChanged(hasFocus)
    if (hasFocus) {
      hideSystemBars()
      focusWebView()
    }
  }

  private fun hideSystemBars() {
    val c = WindowCompat.getInsetsController(window, window.decorView)
    c.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
    c.hide(WindowInsetsCompat.Type.systemBars())
  }

  // Hat and stick input don't leave touch mode, so the WebView needs focus requested.
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
      if (event.action == KeyEvent.ACTION_DOWN && (dir != null || event.repeatCount == 0)) send(name)
      return true
    }
    return super.dispatchKeyEvent(event)
  }

  // Hat and stick directions, auto-repeated while held.
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

    // Script-driven focus doesn't open the keyboard.
    @JavascriptInterface
    fun showKeyboard() = main.post {
      webView?.let {
        if (!it.hasFocus()) it.requestFocus()
        (getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).showSoftInput(it, InputMethodManager.SHOW_IMPLICIT)
      }
    }

    @JavascriptInterface
    fun exit() = main.post { finish() }

    @JavascriptInterface
    fun pickFolder(id: String) = main.post { pendingId = id; treePicker.launch(null) }

    @JavascriptInterface
    fun pickFile(id: String) = main.post { pendingId = id; filePicker.launch(arrayOf("application/json", "text/plain", "application/octet-stream")) }

    @JavascriptInterface
    fun cacheDir(): String = this@MainActivity.cacheDir.absolutePath

    // Paths must be allowed by res/xml/file_paths.xml.
    @JavascriptInterface
    fun share(path: String, mime: String, subject: String) = main.post {
      try {
        val uri = FileProvider.getUriForFile(this@MainActivity, "$packageName.fileprovider", File(path))
        val send = Intent(Intent.ACTION_SEND).apply {
          type = mime
          putExtra(Intent.EXTRA_STREAM, uri)
          putExtra(Intent.EXTRA_SUBJECT, subject)
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        startActivity(Intent.createChooser(send, subject))
      } catch (e: Exception) {
        reply("share-error", e.message ?: "Couldn't share that file")
      }
    }

    @JavascriptInterface
    fun keepAwake(on: Boolean) = main.post {
      if (on) window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
      else window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }

    @JavascriptInterface
    fun haptic() = main.post { webView?.performHapticFeedback(HapticFeedbackConstants.CONFIRM) }

    @JavascriptInterface
    fun takeLaunchAction(): String = launchAction.also { launchAction = "" }
  }
}
