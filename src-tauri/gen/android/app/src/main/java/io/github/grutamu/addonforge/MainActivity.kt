package io.github.grutamu.addonforge

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Environment
import android.provider.Settings

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    // AddonForge reads and writes <game folder>/_classic_beta_/Interface/AddOns
    // directly, which needs "All files access". Ask once on launch if missing.
    if (!Environment.isExternalStorageManager()) {
      val intent = Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:$packageName"))
      try {
        startActivity(intent)
      } catch (e: Exception) {
        startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
      }
    }
  }
}
