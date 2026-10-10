# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile
# ML Kit (the QR scanner) creates its components by reflection: release shrinking must keep them whole
# (0.4.0 release: "NoSuchMethodException ... BarcodeRegistrar.<init>" without this)
-keep class com.google.mlkit.** { *; }
-keep class com.google.android.gms.internal.mlkit_vision_** { *; }
# our own bridges are reached from the page and from the core by name
-keep class io.github.m1sstak3.talon.AppBridge { *; }
-keep class io.github.m1sstak3.talon.SecretClip { *; }
