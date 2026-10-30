# The UI calls MainActivity.Bridge by name.
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}
