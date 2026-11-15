# UniFFI's generated JNA bridge resolves Rust symbols by their generated names.
-keep class com.ember.mobile.core.** { *; }
-keep class uniffi.** { *; }

# JNA maps native functions and callbacks reflectively.
-keep class com.sun.jna.** { *; }
-keepattributes *Annotation*

# JNA also ships desktop AWT adapters that are unavailable and unused on Android.
-dontwarn java.awt.**
